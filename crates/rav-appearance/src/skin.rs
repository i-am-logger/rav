//! Skins: the shapes a bar is made of, as data.
//!
//! A theme says what colour a height is; a skin says what shape it is. The two
//! travel together and vary independently, which is why they are separate: the
//! same shapes in someone else's colours is a reskin, and the same colours in
//! someone else's shapes is a different instrument.
//!
//! # What the core is told
//!
//! Only [`steps`](Skin::steps). A skin declares that it offers eight, and
//! `rav_core::Level::fill` answers "four whole rungs and step five of the next"
//! without ever learning that step five is `▅`. Swapping an eight-step skin for
//! a sixteen-step one changes one integer and no arithmetic.
//!
//! # What a surface is told
//!
//! [`Shapes`], which decides whether a partial rung can be *clipped* from the
//! full one or needs its own drawing. That distinction is not decoration - see
//! its documentation - and getting it wrong makes the shaded styles
//! unreproducible.
//!
//! # What an LED strip is told
//!
//! Nothing but the count. A strip has no shapes, so a partial rung becomes a
//! partial *brightness* and `steps` is how many levels of it are worth asking
//! for. That is the same number, read differently - which is the whole reason
//! the core deals in counts.
//!
//! # Not yet read
//!
//! Every preset names a skin and nothing looks at one, and none carries
//! artwork. The terminal draws from `BarStyle`, which names its glyphs
//! directly; a [`Skin`] carries only the step count and leaves the glyphs to
//! the surface. That difference is what would let one setting dress a glyph
//! grid, a pixel surface and an LED strip at once.

/// How a skin covers a partial rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shapes {
    /// One shape, clipped to the fill height.
    ///
    /// What most skins want, and what scales to any resolution without a shape
    /// per step. A bar at five eighths is the full shape with three eighths
    /// taken off the top.
    Clipped,
    /// A distinct shape per step.
    ///
    /// Necessary rather than merely available. rav's block styles include
    /// `░▒▓`, which are three *densities of a whole cell* rather than three
    /// heights - there is no clip of `█` that yields `▒`. A skin that
    /// misdeclares itself as clipped loses them silently, and the shaded styles
    /// are part of what the first pixel release has to reproduce.
    Discrete,
}

/// One rung of the ladder a bar is built from, as a fraction of its pitch.
///
/// Fractions rather than lengths, so the same skin describes a cell sixty
/// device pixels tall, a cell of eight, and one light on a strip. The surface
/// multiplies by whatever a rung is worth to it.
///
/// Transcribed from what WezTerm actually draws rather than from the font: it
/// renders these itself with `custom_block_glyphs`, so the outlines in a font
/// file are not what anyone is looking at. `▇` is the lower seven eighths, `━`
/// is a stroke across the middle, and `░▒▓` are a whole cell at flat opacity -
/// **not** the dithers their outlines describe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rung {
    /// How much of the rung's height the lit shape fills. 1.0 is all of it.
    pub covers: f32,
    /// Where the shape's bottom sits, measured up from the rung's bottom.
    ///
    /// Zero for every ladder that grows off the floor, which is all of them but
    /// the rule: `━` is a stroke at mid-height and has to float.
    pub floats: f32,
    /// Flat opacity of the lit shape.
    ///
    /// The shaded styles are the reason this exists. `▓` is a whole cell at
    /// three quarters, and no clip of a full block yields it.
    pub opacity: f32,
}

impl Rung {
    /// The ordinary case: a shape that fills its rung, opaque, off the floor.
    pub const FULL: Self = Self {
        covers: 1.0,
        floats: 0.0,
        opacity: 1.0,
    };

    /// The lower `covers` of a rung, opaque - the partial block ladder.
    pub const fn lower(covers: f32) -> Self {
        Self {
            covers,
            floats: 0.0,
            opacity: 1.0,
        }
    }

    /// A whole rung at flat opacity - the shaded styles.
    pub const fn shaded(opacity: f32) -> Self {
        Self {
            covers: 1.0,
            floats: 0.0,
            opacity,
        }
    }
}

/// The shapes a bar is drawn from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skin {
    pub name: &'static str,
    /// How many steps a rung divides into. The one thing the core is told.
    steps: usize,
    pub shapes: Shapes,
    /// What one rung looks like. A surface with rungs of its own - a cell grid -
    /// already draws this with a glyph; a surface without them draws it here.
    pub rung: Rung,
}

impl Skin {
    /// At least one step: a skin offering none could not draw a lit rung at all,
    /// and every division of a rung is by this number.
    pub const fn new(name: &'static str, steps: usize, shapes: Shapes) -> Self {
        Self {
            name,
            steps: if steps == 0 { 1 } else { steps },
            shapes,
            rung: Rung::FULL,
        }
    }

    /// The same skin, drawn from a rung of a different shape.
    pub const fn drawn_as(self, rung: Rung) -> Self {
        Self { rung, ..self }
    }

    pub const fn steps(&self) -> usize {
        self.steps
    }

    /// Whether a partial rung needs its own shape rather than a clip of the
    /// whole one.
    pub const fn needs_a_shape_per_step(&self) -> bool {
        matches!(self.shapes, Shapes::Discrete)
    }
}

/// The Unicode block ladder, which is what rav has always drawn.
///
/// Eight steps because `▁▂▃▄▅▆▇█` divide a cell into eighths, and discrete
/// because the same family carries `░▒▓`.
pub const BLOCKS: Skin = Skin::new("blocks", 8, Shapes::Discrete);

/// A plain bar with no sub-rung detail - one lit shape, clipped.
///
/// What an LED strip is, and what a pixel surface can draw at any size without
/// a shape per step.
///
/// Not to be confused with [`ladders::SOLID`], which is the terminal bar style
/// `▇` - the lower seven eighths of every rung. This one has no rungs at all.
/// They were both called "solid" for a day and that was one day too many.
pub const PLAIN: Skin = Skin::new("plain", 1, Shapes::Clipped);

/// The six ladders rav offers, as the terminal's `b` key cycles them.
///
/// Every one is a whole cell or a fraction of one - which is why they need no
/// artwork and no parser, and why a surface that is not made of cells can draw
/// them from these numbers alone. Anything richer is a skin with shapes in it,
/// and arrives with them.
///
/// The fractions come from what WezTerm renders, not from a font: `wezterm
/// ls-fonts` reports every one of these as `drawn by wezterm because
/// custom_block_glyphs=true`, so the font's outlines are not on screen. The
/// difference matters most for the shades, which are flat opacity here and
/// genuine dithers in a font file.
pub mod ladders {
    use super::{Rung, Shapes, Skin};

    /// `█` - the whole cell. The original's look, and the smoothest, because
    /// `▁▂▃▄▅▆▇` give it eight steps within a cell.
    pub const BLOCKS: Skin = Skin::new("blocks", 8, Shapes::Discrete);

    /// `▓` - a whole cell at three quarters, with `░▒` beneath it.
    ///
    /// Three steps, not eight: Unicode has no shaded eighth-blocks, so the top
    /// of a shade bar fades in rather than rising.
    pub const SHADE: Skin = Skin::new("shade", 4, Shapes::Discrete).drawn_as(Rung::shaded(0.75));

    /// `▇` - the lower seven eighths. The densest that still shows a seam.
    pub const SOLID: Skin = Skin::new("solid", 1, Shapes::Clipped).drawn_as(Rung::lower(7.0 / 8.0));

    /// `▆` - the lower three quarters. Near-solid with a thin dark separator.
    pub const THICK: Skin = Skin::new("thick", 1, Shapes::Clipped).drawn_as(Rung::lower(0.75));

    /// `▄` - the lower half. Visibly striped.
    pub const HALF: Skin = Skin::new("half", 1, Shapes::Clipped).drawn_as(Rung::lower(0.5));

    /// `━` - a thin rule at mid-height. The airiest ladder.
    ///
    /// The one that floats. WezTerm draws it as a stroke across the middle of
    /// the cell, so it neither sits on the rung's floor nor fills it - and a
    /// ladder of these reads as rungs with air between them rather than as a
    /// bar with gaps cut out.
    pub const LINE: Skin = Skin::new("line", 1, Shapes::Clipped).drawn_as(Rung {
        covers: 1.0 / 8.0,
        floats: 7.0 / 16.0,
        opacity: 1.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rav_core::units::Level;

    #[test]
    fn a_skin_tells_the_core_only_how_many_steps_it_has() {
        // The boundary, as arithmetic: the fill is computed from a count, and
        // nothing about a glyph reaches it.
        let fill = Level::new(0.5).fill(8, BLOCKS.steps());
        assert_eq!(fill.whole, 4);
        assert_eq!(fill.part.count(), 8, "the skin's own resolution");
    }

    #[test]
    fn swapping_a_skin_changes_one_integer_and_no_arithmetic() {
        // An eight-step ladder and a sixteen-step one differ here and nowhere
        // else, which is what makes a skin swappable at all.
        let fine = Skin::new("fine", 16, Shapes::Discrete);
        let level = Level::new(0.5);
        assert_eq!(
            level.fill(8, BLOCKS.steps()).whole,
            level.fill(8, fine.steps()).whole,
            "the same level fills the same whole rungs either way"
        );
        assert_ne!(BLOCKS.steps(), fine.steps());
    }

    #[test]
    fn a_shaded_skin_must_declare_itself_discrete() {
        // The distinction that is not decoration. There is no clip of a full
        // cell that yields a half-density one, so a skin carrying the shades
        // cannot be drawn by clipping.
        assert!(BLOCKS.needs_a_shape_per_step());
        assert!(!PLAIN.needs_a_shape_per_step());
    }

    #[test]
    fn a_strip_reads_the_same_count_as_brightness() {
        // No shapes on an LED bar, so the partial rung becomes partial power -
        // and on four lights that partial rung is a quarter of everything the
        // viewer can see, which is why the count travels rather than the shape.
        let fill = Level::new(0.6).fill(4, BLOCKS.steps());
        assert_eq!(fill.whole, 2, "two lights fully on");
        assert!(fill.has_part(), "and the third partly lit");
        // Rescaled onto a driver with its own resolution, unchanged in meaning.
        assert_eq!(fill.part.rescaled(256).count(), 256);
    }

    #[test]
    fn a_skin_with_no_steps_is_not_representable() {
        // Every division of a rung is by this number, so zero would be a
        // division by zero at the point of drawing.
        assert_eq!(Skin::new("empty", 0, Shapes::Clipped).steps(), 1);
    }
}
