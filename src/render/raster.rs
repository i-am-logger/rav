//! Turning a scene into pixels.
//!
//! The pixel surface, and the only place in rav that owns a rasteriser. The
//! scene it draws, the colours it draws in and the geometry it places are all
//! from crates that have never heard of tiny-skia - which is what lets the same
//! scene go to a glyph grid or an LED matrix instead.

use crate::render::{Colour, Ramp, Scene};
use rav_core::geometry::{Rectangle, Screen};
use rav_core::transform::Transform;
use rav_core::units::Length;
use tiny_skia::{Paint, PathBuilder, Pixmap, Rect as SkRect, Transform as SkTransform};

/// An RGBA buffer that geometry is drawn into.
///
/// Owns a `tiny_skia::Pixmap` rather than exposing it, because the pixels leave
/// here in straight alpha and tiny-skia stores them premultiplied - a caller
/// handed the raw buffer would send subtly wrong edges to the terminal and have
/// no reason to suspect it.
pub struct Canvas {
    pixmap: Pixmap,
    placing: Transform,
    antialias: bool,
}

impl Canvas {
    /// `None` for a zero dimension, which is a terminal mid-resize rather than a
    /// programming error, and is the caller's to skip.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Pixmap::new(width, height).map(|pixmap| Self {
            pixmap,
            placing: Transform::IDENTITY,
            antialias: true,
        })
    }

    /// Draw everything from here on through a transform - leaning, turning or
    /// receding, per its own documentation.
    ///
    /// Set on the canvas rather than passed to each fill because it is a
    /// property of how the picture is being looked at, not of one bar. A scene
    /// that never calls this is drawn exactly as it was before transforms
    /// existed, down to the byte - see [`Self::fill`].
    pub fn place_through(&mut self, placing: Transform) {
        self.placing = placing;
    }

    pub fn placing(&self) -> Transform {
        self.placing
    }

    pub fn antialias(&mut self, on: bool) {
        self.antialias = on;
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
    ///
    /// A canvas with no transform takes the rectangle straight to `fill_rect`,
    /// which is not a shortcut but the point: measured, the same rectangle sent
    /// as a four-point path differs in 62 bytes of 38,400, by up to 2 of 255,
    /// along the antialiased edges. Small, and still a changed picture. Keeping
    /// the untransformed path untouched means today's frame is today's frame by
    /// construction rather than within a tolerance somebody chose.
    pub fn fill(&mut self, area: Rectangle, colour: Colour) {
        self.fill_many(&[area], colour);
    }

    /// Paint several rectangles in the same colour, in one fill.
    ///
    /// Worth about a fifth of an angled frame, and no more than that - the
    /// measurement is here because the obvious reading of it is wrong. Eighty
    /// tall quads at 2400x1440 cost **5.595 ms** filled one at a time and
    /// **4.474 ms** batched into a single path, which looks like per-call
    /// overhead and is not: a batched path spans the whole picture, so every
    /// scanline still walks every quad's edges. The same eighty without
    /// antialiasing cost **0.802 ms** and **0.709 ms**. Smoothing the edges is
    /// what an angled view actually pays for; see [`Draw::draw`], which spends
    /// it on the bars and not on the backdrop.
    ///
    /// Callers may only batch shapes that cannot occlude each other - the same
    /// colour, or laid out on one plane - because a single fill has no order
    /// inside it.
    pub fn fill_many(&mut self, areas: &[Rectangle], colour: Colour) {
        if areas.is_empty() {
            return;
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(colour.red, colour.green, colour.blue, colour.alpha);
        // Antialiasing on, which is the whole argument for drawing pixels: a bar
        // edge lands where the arithmetic puts it instead of being snapped to a
        // boundary, and that snapping is the mechanism behind #63.
        paint.anti_alias = self.antialias;

        if self.placing.is_identity() {
            for area in areas {
                // The one place a distance stops being a `Length` and becomes
                // the number tiny-skia takes, so the unit cannot be lost above
                // here.
                let Some(rect) = SkRect::from_xywh(
                    area.left().get(),
                    area.top().get(),
                    area.width().get(),
                    area.height().get(),
                ) else {
                    continue;
                };
                self.pixmap
                    .fill_rect(rect, &paint, SkTransform::identity(), None);
            }
            return;
        }

        let mut path = PathBuilder::new();
        for area in areas {
            if area.is_empty() {
                continue;
            }
            // A corner with no place on the picture takes that shape with it:
            // three corners of a bar is not a bar, and half of one is worse
            // than none.
            let Some(quad) = self.placing.quad(*area) else {
                continue;
            };
            for (at, corner) in quad.corners.iter().enumerate() {
                let (x, y) = (corner.x.get(), corner.y.get());
                if at == 0 {
                    path.move_to(x, y);
                } else {
                    path.line_to(x, y);
                }
            }
            path.close();
        }
        // `finish` refuses a path with no area, which a bar edge-on to the
        // viewer is - and a scene turned a quarter turn has every bar that way.
        let Some(path) = path.finish() else {
            return;
        };
        self.pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            SkTransform::identity(),
            None,
        );
    }

    /// Paint one rectangle in whatever colour the ramp gives at each height.
    ///
    /// Stripe by stripe rather than row by row, so the cost is the number of
    /// stops and not the height of the rectangle.
    pub fn fill_ramped(&mut self, area: Rectangle, ramp: &Ramp, screen: &Screen) {
        self.fill_ramped_at(area, ramp, screen, 1.0);
    }

    /// The same, at a flat opacity over whatever the ramp gives.
    ///
    /// What the shaded styles are. `▓` is a whole cell at three quarters, and
    /// scaling the ramp's own alpha is how that reaches a surface with no
    /// glyph to carry it - the colour still comes from the height, so the bar
    /// still reddens as it rises, just fainter all the way up.
    pub fn fill_ramped_at(&mut self, area: Rectangle, ramp: &Ramp, screen: &Screen, opacity: f32) {
        self.fill_many_ramped_at(&[area], ramp, screen, opacity);
    }

    /// Several rectangles through the same ramp, one fill per colour instead of
    /// one per colour per rectangle.
    ///
    /// A stripe is a band of the screen's own height, so the piece each
    /// rectangle contributes to it is the same colour as every other's - which
    /// is what makes them safe to batch even where they overlap. See
    /// [`Self::fill_many`] for why it matters; a backdrop pass at eighty bands
    /// is the case it was written for.
    pub fn fill_many_ramped_at(
        &mut self,
        areas: &[Rectangle],
        ramp: &Ramp,
        screen: &Screen,
        opacity: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        // Reused down the stripes rather than allocated per stripe: this runs
        // once per band per frame at sixty frames a second.
        let mut pieces: Vec<Rectangle> = Vec::with_capacity(areas.len());
        for stripe in ramp.stripes(screen.height()) {
            pieces.clear();
            pieces.extend(areas.iter().filter_map(|area| stripe.clip(*area)));
            if pieces.is_empty() {
                continue;
            }
            let colour = if opacity >= 1.0 {
                stripe.colour
            } else {
                Colour {
                    alpha: (f32::from(stripe.colour.alpha) * opacity) as u8,
                    ..stripe.colour
                }
            };
            self.fill_many(&pieces, colour);
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

/// Drawing a scene onto a surface.
///
/// An extension trait rather than an inherent method, because [`Scene`] belongs
/// to `rav-appearance` and that crate must never name a rasteriser: a scene that
/// knew how to rasterise itself would be one only a rasteriser could consume,
/// and an LED matrix has none. The trait is where the pixel surface adds the
/// ability without the scene acquiring it.
pub trait Draw {
    fn draw(&self, canvas: &mut Canvas);
}

impl Draw for Scene<'_> {
    /// Back to front: backdrop, then bars, then caps - the order that lets a cap
    /// lie over the backdrop instead of replacing it. Each layer runs over every
    /// band before the next begins, so a band's cap is never buried by its
    /// neighbour's backdrop when the two wear different styles.
    fn draw(&self, canvas: &mut Canvas) {
        canvas.clear();
        let drawable = self.visible_bands();
        let screen = &self.screen;

        // Computed once: it costs a rotation and a projection of the field's own
        // corners, and it is the same for every band. Only the depth differs.
        let placing = self.view.placing_at(screen, self.elapsed);

        let stand = |canvas: &mut Canvas, index: usize| {
            let depth = self.view.depth(index, drawable, screen);
            canvas.place_through(if depth == 0.0 {
                placing
            } else {
                Transform::moving(0.0, 0.0, depth).then(placing)
            });
        };
        // Antialiasing is what an angled edge costs: measured at 2400x1440,
        // eighty tall quads take 5.595 ms smoothed and 0.802 ms not - seven
        // times, for the same ink, and it is the whole of the difference
        // between an angled view that holds sixty frames a second and one that
        // does not. The backdrop goes without it: a dim field standing behind
        // the bars, most of it covered by them, and none of its edges are what
        // anyone is looking at. The bars and the caps keep it, because an edge
        // landing where the arithmetic puts it rather than on a pixel boundary
        // is the whole argument for drawing pixels at all.
        //
        // Only ever while angled. Flat draws rectangles through a route that
        // smooths them for nothing, so today's picture keeps every edge it has.
        let angled = !placing.is_identity() || self.view.recedes();
        let backdrop = |canvas: &mut Canvas, index: usize| {
            let band = &self.bands[index];
            if let Some(grid) = self.style_of(band).and_then(|style| style.grid) {
                let area = self.layout.column(index).backdrop(screen);
                canvas.antialias(!angled);
                canvas.fill_ramped(area, grid, screen);
                canvas.antialias(true);
            }
        };
        let bar = |canvas: &mut Canvas, index: usize| {
            let band = &self.bands[index];
            if let Some(style) = self.style_of(band) {
                let area = self.layout.column(index).bar(band.level, screen);
                match style.ladder {
                    Some((skin, rung)) => draw_ladder(canvas, area, skin, rung, style.bars, screen),
                    None => canvas.fill_ramped(area, style.bars, screen),
                }
            }
        };
        let cap = |canvas: &mut Canvas, index: usize| {
            let band = &self.bands[index];
            if let Some(cap) = self.style_of(band).and_then(|style| style.cap) {
                let column = self.layout.column(index);
                canvas.fill(column.cap(band.peak, cap.thickness, screen), cap.colour);
            }
        };

        if self.view.recedes() {
            // A whole band at a time, furthest first. Once the bands are at
            // different depths, near over far is the only order that means
            // anything - and it has to hold across the three layers, not merely
            // inside each: a far *cap* drawn in a later pass would land on top
            // of a near *bar* drawn in an earlier one, which is a distant peak
            // hanging in front of the loudest band in the picture.
            //
            // Counted rather than collected: a `Vec` of indices per frame is an
            // allocation in the one loop that runs sixty times a second.
            for step in 0..drawable {
                let index = drawable - 1 - step;
                stand(canvas, index);
                backdrop(canvas, index);
                bar(canvas, index);
                cap(canvas, index);
            }
            return;
        }

        // On one plane nothing can overlap, so there is no depth for an order to
        // respect and a different rule takes over: every backdrop before any
        // bar, and every bar before any cap. That is what keeps a band's cap
        // clear of its neighbour's backdrop when the two wear different styles.
        //
        // And because nothing overlaps, a whole layer can go down in one fill
        // per colour instead of one per colour per band - which is the
        // difference between an angled view that holds sixty frames a second
        // and one that does not. See [`Canvas::fill_many`].
        stand(canvas, 0);
        let wears = |index: usize| {
            // `style_of`'s rule, as an index: a band naming a style that is not
            // there is drawn in the first one.
            let named = self.bands[index].style.index();
            if named < self.styles.len() { named } else { 0 }
        };
        let mut areas: Vec<Rectangle> = Vec::with_capacity(drawable);
        let mut wearers: Vec<usize> = Vec::with_capacity(drawable);

        for (id, style) in self.styles.iter().enumerate() {
            wearers.clear();
            wearers.extend((0..drawable).filter(|&index| wears(index) == id));
            if wearers.is_empty() {
                continue;
            }
            if let Some(grid) = style.grid {
                areas.clear();
                areas.extend(
                    wearers
                        .iter()
                        .map(|&index| self.layout.column(index).backdrop(screen)),
                );
                canvas.antialias(!angled);
                canvas.fill_many_ramped_at(&areas, grid, screen, 1.0);
                canvas.antialias(true);
            }
        }

        for (id, style) in self.styles.iter().enumerate() {
            wearers.clear();
            wearers.extend((0..drawable).filter(|&index| wears(index) == id));
            if wearers.is_empty() {
                continue;
            }
            match style.ladder {
                // A ladder is rungs, and rung `r` sits at the same height in
                // every band, so it takes the same colour in every band - which
                // is what lets a row of them go down together.
                Some((skin, rung)) => {
                    areas.clear();
                    areas.extend(wearers.iter().map(|&index| {
                        self.layout
                            .column(index)
                            .bar(self.bands[index].level, screen)
                    }));
                    draw_ladders(canvas, &areas, skin, rung, style.bars, screen);
                }
                None => {
                    areas.clear();
                    areas.extend(wearers.iter().map(|&index| {
                        self.layout
                            .column(index)
                            .bar(self.bands[index].level, screen)
                    }));
                    canvas.fill_many_ramped_at(&areas, style.bars, screen, 1.0);
                }
            }
        }
        for (id, style) in self.styles.iter().enumerate() {
            let Some(shape) = style.cap else {
                continue;
            };
            areas.clear();
            areas.extend(
                (0..drawable)
                    .filter(|&index| wears(index) == id)
                    .map(|index| {
                        self.layout.column(index).cap(
                            self.bands[index].peak,
                            shape.thickness,
                            screen,
                        )
                    }),
            );
            canvas.fill_many(&areas, shape.colour);
        }
    }
}

/// A row of bars as the ladders their skin describes, a rung at a time.
///
/// Rung `r` sits at the same height in every bar, so every bar's `r` takes the
/// same colour from the ramp - which is what makes a row of them one fill
/// rather than one each. See [`Canvas::fill_many`] for why that is the
/// difference between holding sixty frames a second and not.
fn draw_ladders(
    canvas: &mut Canvas,
    bars: &[Rectangle],
    skin: rav_appearance::Skin,
    rung: Length,
    ramp: &Ramp,
    screen: &Screen,
) {
    if rung.get() <= 0.0 {
        canvas.fill_many_ramped_at(bars, ramp, screen, 1.0);
        return;
    }
    let shape = skin.rung;
    let mut row: Vec<Rectangle> = Vec::with_capacity(bars.len());
    let tallest = bars
        .iter()
        .map(|bar| bar.height().get())
        .fold(0.0f32, f32::max);
    let rungs = ((tallest / rung.get()).ceil() as usize).min(4096);

    for index in 0..rungs {
        row.clear();
        for bar in bars {
            if bar.height().get() <= 0.0 {
                continue;
            }
            let floor = bar.bottom();
            let rung_bottom = floor - rung * (index as f32);
            let shape_bottom = rung_bottom - rung * shape.floats;
            let shape_top = shape_bottom - rung * shape.covers;
            // Clipped to the bar, so the rung a bar stops inside is drawn only
            // as far as the bar reached.
            let top = shape_top.largest(bar.top());
            let bottom = shape_bottom.smallest(floor);
            if bottom > top {
                row.push(Rectangle::new(bar.left(), top, bar.width(), bottom - top));
            }
        }
        if row.is_empty() {
            continue;
        }
        if shape.opacity >= 1.0 {
            canvas.fill_many_ramped_at(&row, ramp, screen, 1.0);
        } else {
            canvas.fill_many_ramped_at(&row, ramp, screen, shape.opacity);
        }
    }
}

/// Draw a bar as the ladder its skin describes, rung by rung.
///
/// The bar is the same rectangle either way; what changes is what fills it. A
/// solid style covers the lower seven eighths of each rung, so the ladder shows
/// a seam every rung; a rule covers an eighth in the middle of one and reads as
/// air. That is what `b` chooses, and on a cell grid the glyph does it - here
/// nothing does unless this does.
///
/// The topmost rung is clipped to the bar rather than drawn whole, so a bar
/// that stops part way up a rung stops there instead of rounding up to it.
fn draw_ladder(
    canvas: &mut Canvas,
    bar: Rectangle,
    skin: rav_appearance::Skin,
    rung: Length,
    ramp: &Ramp,
    screen: &Screen,
) {
    // A rung with no height is a division by zero waiting to happen, and a
    // screen mid-resize produces one. One shape is the honest fallback: it is
    // what the ladder collapses to when there is no room for rungs.
    if rung.get() <= 0.0 || bar.height().get() <= 0.0 {
        canvas.fill_ramped(bar, ramp, screen);
        return;
    }

    let shape = skin.rung;
    let floor = bar.bottom();
    let mut lit = bar.height().get();
    let mut index = 0usize;

    while lit > 0.0 {
        // Rungs are counted from the floor, which is where a bar grows from.
        let rung_bottom = floor - rung * (index as f32);
        let rung_top = rung_bottom - rung;

        let shape_bottom = rung_bottom - rung * shape.floats;
        let shape_top = shape_bottom - rung * shape.covers;

        // Clipped to the bar, so the rung the bar stops inside is drawn only as
        // far as the bar reached.
        let top = shape_top.largest(bar.top());
        let bottom = shape_bottom.smallest(floor);
        if bottom > top {
            let piece = Rectangle::new(bar.left(), top, bar.width(), bottom - top);
            if shape.opacity >= 1.0 {
                canvas.fill_ramped(piece, ramp, screen);
            } else {
                canvas.fill_ramped_at(piece, ramp, screen, shape.opacity);
            }
        }

        lit -= rung.get();
        index += 1;
        // Nothing draws below the floor or above the bar, so the loop is bounded
        // by the bar's own height - but a rung smaller than float precision
        // would not shrink `lit`, and this is the render path.
        if index > 4096 || rung_top.get() < bar.top().get() - rung.get() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Band, CapStyle, Scene, Style, StyleId};
    use rav_core::geometry::BarLayout;
    use rav_core::units::{Length, Level};

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

    /// One style, with caps on unless `capped` says otherwise.
    fn one_style<'a>(bars: &'a Ramp, grid: Option<&'a Ramp>, capped: bool) -> [Style<'a>; 1] {
        [Style {
            bars,
            grid,
            cap: capped.then_some(CapStyle {
                colour: Colour::WHITE,
                thickness: Length(3.0),
            }),
            ladder: None,
        }]
    }

    fn scene<'a>(bands: &'a [Band], styles: &'a [Style<'a>]) -> Scene<'a> {
        Scene {
            bands,
            view: rav_appearance::View::Flat,
            elapsed: rav_core::units::Elapsed::seconds(0.0),
            layout: layout(10.0, 0.0),
            screen: screen(10.0, 60.0),
            styles,
        }
    }

    /// A bar in the middle of a small canvas, for the transform tests.
    fn a_bar() -> Rectangle {
        Rectangle::new(Length(20.0), Length(20.0), Length(40.0), Length(60.0))
    }

    #[test]
    fn a_canvas_nobody_transformed_draws_what_it_always_drew() {
        // The whole of what makes this safe to add. `fill_rect` and a four-point
        // path are not the same pixels - measured at 62 bytes of 38,400,
        // differing by up to 2 of 255 along the antialiased edges - so the
        // untransformed picture keeps the code path it has always had rather
        // than a tolerance. Every other test in this file is the rest of the
        // guard: they all draw through `fill` and none of them moved.
        let screen = screen(120.0, 100.0);
        let mut untouched = Canvas::for_screen(&screen).unwrap();
        untouched.fill(a_bar(), Colour::GREEN);

        let mut told_to_do_nothing = Canvas::for_screen(&screen).unwrap();
        told_to_do_nothing.place_through(Transform::IDENTITY);
        told_to_do_nothing.fill(a_bar(), Colour::GREEN);

        assert!(untouched.placing().is_identity());
        assert_eq!(
            untouched.to_rgba(),
            told_to_do_nothing.to_rgba(),
            "setting the transform that means nothing changed the picture",
        );
    }

    #[test]
    fn a_transform_moves_the_picture_and_not_the_layout() {
        // The bar is laid out in the same place either way - what changed is
        // where the canvas puts it. A skin leans; the analyser does not learn
        // about leaning.
        let screen = screen(120.0, 100.0);
        let mut flat = Canvas::for_screen(&screen).unwrap();
        flat.fill(a_bar(), Colour::GREEN);

        let mut shifted = Canvas::for_screen(&screen).unwrap();
        shifted.place_through(Transform::moving(30.0, 0.0, 0.0));
        shifted.fill(a_bar(), Colour::GREEN);

        assert_eq!(at(&flat, 30, 50).green, 255, "the bar was not where it was");
        assert_eq!(
            at(&flat, 70, 50).alpha,
            0,
            "something is already over there"
        );
        assert_eq!(at(&shifted, 70, 50).green, 255, "it did not move");
        assert_eq!(at(&shifted, 30, 50).alpha, 0, "it is in both places");
    }

    #[test]
    fn a_receding_bar_is_smaller_at_the_far_end() {
        // Depth reading as depth, which is the point of the exercise: the same
        // rectangle, pushed back, covers less of the picture. Compared by ink
        // rather than by an edge, because an edge under perspective is
        // antialiased across a pixel and its exact position is a choice the
        // rasteriser makes.
        let screen = screen(120.0, 100.0);
        let ink = |placing: Transform| {
            let mut canvas = Canvas::for_screen(&screen).unwrap();
            canvas.place_through(placing);
            canvas.fill(a_bar(), Colour::GREEN);
            canvas
                .to_rgba()
                .chunks(4)
                .map(|pixel| u32::from(pixel[3]))
                .sum::<u32>()
        };

        let near = ink(Transform::receding(400.0));
        let far = ink(Transform::moving(0.0, 0.0, 200.0).then(Transform::receding(400.0)));
        assert!(
            far * 2 < near,
            "pushed back twice as far as the eye is near, and it covered {far} against {near}",
        );
        assert!(far > 0, "it receded out of existence");
    }

    #[test]
    fn a_bar_turned_edge_on_draws_nothing_rather_than_a_seam() {
        // A quarter turn puts every bar side-on. `PathBuilder::finish` refuses a
        // path with no area, and taking that as "nothing to draw" is the
        // difference between a scene that fades out and one that leaves a line
        // of stray pixels down the screen.
        let screen = screen(120.0, 100.0);
        let mut canvas = Canvas::for_screen(&screen).unwrap();
        canvas.place_through(Transform::turning(core::f32::consts::FRAC_PI_2));
        canvas.fill(a_bar(), Colour::GREEN);
        assert!(
            canvas.to_rgba().chunks(4).all(|pixel| pixel[3] == 0),
            "an edge-on bar left ink behind",
        );
    }

    #[test]
    fn a_bar_behind_the_viewer_draws_nothing() {
        // Not a contrived case once a scene turns: half a turned field is behind
        // the eye. Projecting it anyway puts the bar in the reflection of where
        // it belongs, which reads as a glitch rather than as depth.
        let screen = screen(120.0, 100.0);
        let mut canvas = Canvas::for_screen(&screen).unwrap();
        canvas.place_through(Transform::moving(0.0, 0.0, -200.0).then(Transform::receding(100.0)));
        canvas.fill(a_bar(), Colour::GREEN);
        assert!(
            canvas.to_rgba().chunks(4).all(|pixel| pixel[3] == 0),
            "a bar behind the eye was drawn somewhere",
        );
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
        scene(&quiet, &one_style(&ramp, None, true)).draw(&mut short);
        let mut tall = Canvas::new(10, 60).unwrap();
        scene(&loud, &one_style(&ramp, None, true)).draw(&mut tall);

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
    fn in_a_corridor_the_near_band_is_drawn_over_the_far_one() {
        // The first thing in rav that needs a drawing order at all. A fence of
        // bars on one plane cannot overlap, so the three passes could run in any
        // order and did; put the bands at different depths and the far one
        // shrinks toward the middle of the picture, straight across its
        // neighbour. Near over far is the whole of what a painter has instead of
        // a depth buffer, and backwards it puts the quiet distance on top of the
        // loud foreground.
        let near_ramp = Ramp::new(vec![Colour::GREEN]);
        let far_ramp = Ramp::new(vec![Colour::BLUE]);
        let styles = [
            Style {
                bars: &near_ramp,
                grid: None,
                cap: None,
                ladder: None,
            },
            Style {
                bars: &far_ramp,
                grid: None,
                cap: None,
                ladder: None,
            },
        ];
        // Three columns of forty on a screen of a hundred and twenty, so the
        // middle of the picture falls inside the middle column rather than on a
        // boundary. Two columns either side of the middle shrink toward it by
        // different amounts and genuinely cross: worked out rather than hoped
        // for, because the first version of this used two equal halves, where
        // the far one's near edge lands exactly on the middle and nothing ever
        // overlaps - it passed with the order reversed.
        //
        // At the eye three screens back, the middle band stands at `w = 4/3` and
        // covers 45 to 75; the far one at `w = 5/3` covers 72 to 96. They share
        // 72 to 75, and the middle band is the nearer of the two.
        let three = [
            Band::new(Level::FULL, Level::FULL),
            Band::new(Level::FULL, Level::FULL),
            Band::new(Level::FULL, Level::FULL).styled(StyleId(1)),
        ];
        let screen = screen(120.0, 100.0);
        let scene = Scene {
            bands: &three,
            layout: BarLayout::new(Length(40.0), Length(0.0)),
            screen,
            view: rav_appearance::View::Corridor,
            elapsed: rav_core::units::Elapsed::seconds(0.0),
            styles: &styles,
        };
        let mut canvas = Canvas::for_screen(&screen).unwrap();
        scene.draw(&mut canvas);

        let overlapped = at(&canvas, 73, 50);
        assert_eq!(
            (overlapped.red, overlapped.green, overlapped.blue),
            (0, 255, 0),
            "the far band was painted over the near one",
        );
    }

    #[test]
    fn a_far_cap_does_not_hang_in_front_of_a_near_bar() {
        // Ordering the bands inside each of the three layers is not enough once
        // they are at different depths: caps are drawn after every bar, so a
        // distant peak would land on top of the loudest band in the picture. In
        // a corridor a whole band goes down at a time instead.
        //
        // The same three columns of forty as the test above, so the middle band
        // covers 45 to 75 and the far one 72 to 96. The far band's cap rides at
        // the top of its bar; the middle band is full height and nearer, and
        // owns every pixel the two share.
        let green = Ramp::new(vec![Colour::GREEN]);
        let blue = Ramp::new(vec![Colour::BLUE]);
        let styles = [
            Style {
                bars: &green,
                grid: None,
                cap: None,
                ladder: None,
            },
            Style {
                bars: &blue,
                grid: None,
                cap: Some(CapStyle {
                    colour: Colour::WHITE,
                    thickness: Length(8.0),
                }),
                ladder: None,
            },
        ];
        let three = [
            Band::new(Level::FULL, Level::FULL),
            Band::new(Level::FULL, Level::FULL),
            Band::new(Level::FULL, Level::FULL).styled(StyleId(1)),
        ];
        let screen = screen(120.0, 100.0);
        let mut canvas = Canvas::for_screen(&screen).unwrap();
        Scene {
            bands: &three,
            layout: BarLayout::new(Length(40.0), Length(0.0)),
            screen,
            view: rav_appearance::View::Corridor,
            elapsed: rav_core::units::Elapsed::seconds(0.0),
            styles: &styles,
        }
        .draw(&mut canvas);

        // Down the strip the two bands share, wherever the far cap reaches.
        for y in 20..80 {
            let there = at(&canvas, 73, y);
            assert_ne!(
                (there.red, there.green, there.blue),
                (255, 255, 255),
                "the far band's cap was drawn over the near band's bar, at y {y}",
            );
        }
    }

    #[test]
    fn bands_can_wear_different_styles() {
        // What a stereo pair needs: left and right in their own colours, from
        // one scene, without the core learning what either colour is.
        let left_ramp = Ramp::new(vec![Colour::GREEN]);
        let right_ramp = Ramp::new(vec![Colour::BLUE]);
        let styles = [
            Style {
                bars: &left_ramp,
                grid: None,
                cap: None,
                ladder: None,
            },
            Style {
                bars: &right_ramp,
                grid: None,
                cap: None,
                ladder: None,
            },
        ];
        // Two bands side by side, second one in the second style.
        let pair = [
            Band::new(Level::FULL, Level::FULL),
            Band::new(Level::FULL, Level::FULL).styled(StyleId(1)),
        ];
        let mut canvas = Canvas::new(20, 60).unwrap();
        Scene {
            screen: screen(20.0, 60.0),
            ..scene(&pair, &styles)
        }
        .draw(&mut canvas);

        assert_eq!(at(&canvas, 5, 30), Colour::GREEN, "the first band");
        assert_eq!(at(&canvas, 15, 30), Colour::BLUE, "the second band");
    }

    #[test]
    fn a_style_does_not_break_colour_following_height() {
        // The invariant per-band styling could have broken. Within one style,
        // colour is still a function of height and not of the band's own level,
        // so a quiet band and a loud one wearing the same style still agree
        // where they overlap.
        let ramp = traffic_light();
        let other = Ramp::new(vec![Colour::BLUE]);
        let styles = [
            Style {
                bars: &ramp,
                grid: None,
                cap: None,
                ladder: None,
            },
            Style {
                bars: &other,
                grid: None,
                cap: None,
                ladder: None,
            },
        ];
        // Band 0 quiet, band 1 loud - both in style 0, with style 1 present but
        // unused, so the lookup cannot be what makes them agree.
        let mixed = [
            Band::new(Level::new(0.3), Level::SILENT),
            Band::new(Level::new(0.9), Level::SILENT),
        ];
        let mut canvas = Canvas::new(20, 60).unwrap();
        Scene {
            screen: screen(20.0, 60.0),
            ..scene(&mixed, &styles)
        }
        .draw(&mut canvas);

        for y in 43..60 {
            assert_eq!(
                at(&canvas, 5, y),
                at(&canvas, 15, y),
                "row {y}: same style, same height, so the same colour"
            );
        }
    }

    #[test]
    fn a_band_naming_a_style_that_is_not_there_falls_back() {
        // A stale index after a theme change is a frame in the wrong colour, not
        // a reason to take the process down mid-render.
        let ramp = Ramp::new(vec![Colour::GREEN]);
        let only = one_style(&ramp, None, false);
        let stale = [Band::new(Level::FULL, Level::SILENT).styled(StyleId(7))];
        let mut canvas = Canvas::new(10, 60).unwrap();
        scene(&stale, &only).draw(&mut canvas);
        assert_eq!(at(&canvas, 5, 30), Colour::GREEN, "fell back to the first");
    }

    #[test]
    fn a_scene_with_no_styles_draws_nothing_rather_than_inventing_a_colour() {
        let nothing: [Style; 0] = [];
        let loud = bands(&[1.0]);
        let mut canvas = Canvas::new(10, 60).unwrap();
        scene(&loud, &nothing).draw(&mut canvas);
        assert!(canvas.to_rgba().chunks(4).all(|pixel| pixel[3] == 0));
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
        scene(&silent, &one_style(&ramp, None, false)).draw(&mut canvas);
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
        scene(&loud, &one_style(&ramp, Some(&grid), true)).draw(&mut with);
        let mut without = Canvas::new(10, 60).unwrap();
        scene(&loud, &one_style(&ramp, Some(&grid), false)).draw(&mut without);

        assert_ne!(with.to_rgba(), without.to_rgba(), "a cap must be visible");
    }

    #[test]
    fn no_grid_leaves_the_terminal_background_showing() {
        // A theme without a grid must not invent one - the terminal's own
        // background is what shows through, so those pixels stay transparent.
        let ramp = traffic_light();
        let quiet = bands(&[0.25]);
        let mut canvas = Canvas::new(10, 60).unwrap();
        scene(&quiet, &one_style(&ramp, None, false)).draw(&mut canvas);
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
        scene(&edge_case, &one_style(&ramp, Some(&grid), false)).draw(&mut over_grid);
        let blended = at(&over_grid, 5, 40);
        assert_eq!(blended.alpha, 0xff, "the backdrop keeps the edge opaque");
        assert!(
            blended.red > 0 && blended.blue > 0,
            "the edge should carry bar and backdrop, got {blended:?}"
        );

        // Without a backdrop the same edge reaches the terminal semi-transparent
        // and the terminal composites it. Different pixels, deliberately.
        let mut bare = Canvas::new(10, 60).unwrap();
        scene(&edge_case, &one_style(&ramp, None, false)).draw(&mut bare);
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

        let plain = one_style(&ramp, None, false);
        let mut exact = Canvas::new(30, 60).unwrap();
        Scene {
            screen: wide,
            ..scene(&three, &plain)
        }
        .draw(&mut exact);
        let mut overflowing = Canvas::new(30, 60).unwrap();
        Scene {
            screen: wide,
            ..scene(&ten, &plain)
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
