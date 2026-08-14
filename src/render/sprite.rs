//! A skin that is a drawing rather than a fraction of a rectangle.
//!
//! Every ladder rav has drawn so far is arithmetic: `▇` is the lower seven
//! eighths of a rung, `━` a stroke across the middle. That is enough for the
//! shapes a terminal already has, and it is the whole vocabulary a rectangle
//! offers. A skin with a rounded end, a bevel or a notch cannot be written that
//! way at all, and this is where those come from.
//!
//! # Why it is here and not in `rav-appearance`
//!
//! Parsing SVG needs an allocator, a filesystem-shaped API and about twenty
//! crates. `rav-appearance` is `no_std` and has a check proving it builds for a
//! machine with no operating system, so the artwork cannot live beside the model
//! that describes it. The split is the honest one rather than a compromise: a
//! skin still says *how many steps* it has and *what shape a rung is* in terms
//! anything can read - a terminal picks a character, an LED strip picks a
//! brightness - and a surface that can rasterise may additionally have a picture
//! for it. One that cannot draws what it always drew.
//!
//! # The drawing gives shape, never colour
//!
//! The ramp still decides what colour a height is. That invariant is older than
//! the pixel surface and is the reason a bar reddens as it rises rather than as
//! it gets louder, so a skin is taken as a **mask**: its alpha is the shape, and
//! the fill underneath is whatever the ramp gives at that height. An SVG whose
//! own colours were used would be a skin overriding the theme, which is the one
//! thing the appearance layer exists to prevent.
//!
//! # Rasterised once, not per frame
//!
//! A rung is a fixed size for as long as the window is, so the drawing is turned
//! into pixels when that size changes and blitted after. Parsing and tessellating
//! an SVG sixty times a second is the obvious trap and would cost more than
//! every bar rav draws put together.

use rav_core::geometry::Rectangle;
use tiny_skia::{Mask, MaskType, Pixmap, PixmapPaint, Transform as SkTransform};

/// One skin's artwork, rasterised for a rung of a particular size.
pub struct Sprite {
    art: Pixmap,
    from: *const u8,
    across: u32,
    down: u32,
}

/// Parse that SVG and keep nothing but the verdict, and why if it is no.
///
/// Startup asks before taking the screen. Once the frame loop has it, artwork
/// that will not parse is only a reason to draw the plain ladder - which reads
/// as rav ignoring the flag rather than as the file being wrong, and is why the
/// question is worth asking early where the answer can still be printed.
///
/// The error is carried rather than thrown away because it is the useful half:
/// `usvg` reports where the file stops making sense, and "this is not an SVG"
/// leaves someone to find that themselves in a file they just drew.
pub fn parses(svg: &str) -> Result<(), usvg::Error> {
    usvg::Tree::from_str(svg, &usvg::Options::default()).map(drop)
}

impl Sprite {
    /// Parse an SVG and rasterise it to fill a rung of this size.
    ///
    /// `None` for artwork that will not parse or a rung with no area - a skin
    /// file someone is editing, or a terminal mid-resize. Both are a reason to
    /// draw the plain ladder, not to stop drawing.
    /// `'static` because the identity check below is by address: every skin
    /// here is an `include_str!`, and a borrowed buffer would give the same
    /// pointer to two different drawings once it was reused.
    pub fn drawn(svg: &'static str, across: u32, down: u32) -> Option<Self> {
        if across == 0 || down == 0 {
            return None;
        }
        // No fonts, no raster images, no system anything: a skin is shapes, and
        // the alternative is dragging a font stack into a spectrum analyser.
        let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
        let mut pixmap = Pixmap::new(across, down)?;
        let size = tree.size();
        // Stretched to the rung rather than fitted inside it. A rung is the unit
        // the ladder is built from and its proportions come from the cell, which
        // is the terminal's to choose - so a skin that insisted on its own
        // aspect would leave gaps the theme never asked for.
        let fill =
            SkTransform::from_scale(across as f32 / size.width(), down as f32 / size.height());
        resvg::render(&tree, fill, &mut pixmap.as_mut());
        Some(Self {
            art: pixmap,
            // Which artwork this was drawn from, by address. Every skin here is
            // an `include_str!`, so one `&'static str` per skin and comparing
            // the pointer answers "is this still the same drawing" without
            // comparing kilobytes of XML on every frame.
            from: svg.as_ptr(),
            across,
            down,
        })
    }

    /// Whether this was drawn for a rung of that size.
    ///
    /// The whole of the caching rule: a rung is a fixed size for as long as the
    /// window is, so the answer is yes on every frame but the one after a
    /// resize.
    /// Whether this was drawn from that artwork.
    pub fn is(&self, svg: &'static str) -> bool {
        std::ptr::eq(self.from, svg.as_ptr())
    }

    pub fn fits(&self, across: u32, down: u32) -> bool {
        self.across == across && self.down == down
    }

    /// A whole screen's worth of ladder, as one shape to fill through.
    ///
    /// Every rung position of every column, stamped up the full height rather
    /// than up each bar - because a bar's height changes sixty times a second
    /// and the *grid* it climbs does not. So this is built when the window
    /// changes size and not again, and a frame clips it to whatever the bars
    /// currently reach.
    ///
    /// One mask rather than one per rung: a clip is in the pixmap's own
    /// coordinates, so a per-rung mask would be a screen-sized allocation
    /// apiece - about nineteen hundred of them a frame at eighty bands.
    pub fn ladder(&self, across: u32, down: u32, columns: &[Rectangle], rung: f32) -> Option<Mask> {
        if rung <= 0.0 || across == 0 || down == 0 {
            return None;
        }
        let mut stamped = Pixmap::new(across, down)?;
        let paint = PixmapPaint::default();
        for column in columns {
            let left = column.left().get();
            // From the floor up, which is the way a bar grows and therefore the
            // way the seams have to line up.
            let mut top = down as f32 - rung;
            while top > -rung {
                stamped.draw_pixmap(
                    left as i32,
                    top as i32,
                    self.art.as_ref(),
                    &paint,
                    SkTransform::identity(),
                    None,
                );
                top -= rung;
            }
        }
        Some(Mask::from_pixmap(stamped.as_ref(), MaskType::Alpha))
    }
}

impl Sprite {
    /// Whether any of this drawing would put ink on the screen.
    ///
    /// An SVG can parse perfectly and rasterise to nothing - empty, or drawn
    /// entirely outside its own `viewBox`. The bars are then filled through a
    /// mask with no coverage and vanish, which reads as rav having broken
    /// rather than as the file being wrong.
    ///
    /// Asked of the rasterised sprite rather than of the file, because that is
    /// the only place the answer exists: coverage depends on the rung size, and
    /// artwork too thin to show at one size shows at another.
    pub fn draws_anything(&self) -> bool {
        self.art.pixels().iter().any(|pixel| pixel.alpha() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rav_core::units::Length;

    /// A circle inside its box, which no ladder of rectangles can be.
    const DOT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <circle cx="5" cy="5" r="5" fill="#fff"/></svg>"##;

    /// The whole box, for comparing against.
    const SLAB: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <rect width="10" height="10" fill="#fff"/></svg>"##;

    /// How much of the rung the drawing covers, 0 to 1.
    fn coverage(svg: &'static str, across: u32, down: u32) -> f32 {
        let sprite = Sprite::drawn(svg, across, down).expect("it should draw");
        let one = Rectangle::new(
            Length(0.0),
            Length(0.0),
            Length(across as f32),
            Length(down as f32),
        );
        let mask = sprite
            .ladder(across, down, &[one], down as f32)
            .expect("a ladder of one rung");
        let covered: u32 = mask.data().iter().map(|&a| u32::from(a)).sum();
        covered as f32 / (across * down * 255) as f32
    }

    #[test]
    fn a_drawing_covers_what_it_draws_and_not_the_box_around_it() {
        // The point of the exercise: a shape a rectangle cannot be. A circle
        // inscribed in its box covers pi/4 of it - about 0.785 - and a slab
        // covers all of it. If the mask were the bounding box, both would be 1.
        let dot = coverage(DOT, 64, 64);
        assert!(
            (dot - 0.785).abs() < 0.01,
            "a circle covered {dot} of its box, not about pi/4",
        );
        assert!(
            (coverage(SLAB, 64, 64) - 1.0).abs() < 0.01,
            "a slab is solid"
        );
    }

    #[test]
    fn a_drawing_is_stretched_to_whatever_a_rung_is() {
        // A rung's proportions come from the terminal's cell, which is nobody's
        // choice here. The same drawing has to fill a tall thin rung and a short
        // wide one, so its coverage is the same fraction of either.
        let tall = coverage(DOT, 16, 96);
        let wide = coverage(DOT, 96, 16);
        assert!(
            (tall - wide).abs() < 0.02,
            "the same drawing covered {tall} of a tall rung and {wide} of a wide one",
        );
    }

    #[test]
    fn artwork_that_will_not_parse_is_no_reason_to_stop_drawing() {
        // A skin file someone is halfway through editing. The caller falls back
        // to the plain ladder, which is a picture rather than an error.
        assert!(Sprite::drawn("not an svg at all", 32, 32).is_none());
        assert!(Sprite::drawn(SLAB, 0, 32).is_none(), "a rung with no width");
        assert!(
            Sprite::drawn(SLAB, 32, 0).is_none(),
            "a rung with no height"
        );
    }

    #[test]
    fn a_sprite_knows_when_it_is_the_wrong_size() {
        // The whole caching rule. Rasterising an SVG sixty times a second would
        // cost more than every bar rav draws put together.
        let sprite = Sprite::drawn(SLAB, 30, 60).unwrap();
        assert!(sprite.fits(30, 60));
        assert!(!sprite.fits(30, 61), "a resize has to redraw it");
    }
}

#[cfg(test)]
mod coverage {
    use super::*;

    const REAL: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" preserveAspectRatio="none"><rect width="10" height="10" fill="#fff"/></svg>"##;
    const EMPTY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"></svg>"##;
    const OUTSIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect x="500" y="500" width="10" height="10" fill="#fff"/></svg>"##;

    #[test]
    fn a_drawing_that_covers_nothing_says_so() {
        // Both of these parse. Neither puts a pixel down, and a bar filled
        // through them disappears - which is why the panel has to be able to
        // ask rather than assuming a parsed file draws.
        for (name, svg) in [("empty", EMPTY), ("outside its viewBox", OUTSIDE)] {
            let sprite = Sprite::drawn(svg, 30, 60).expect("it parses");
            assert!(
                !sprite.draws_anything(),
                "{name} reported coverage it does not have"
            );
        }

        let real = Sprite::drawn(REAL, 30, 60).expect("it parses");
        assert!(real.draws_anything(), "a solid rung reported no coverage");
    }
}
