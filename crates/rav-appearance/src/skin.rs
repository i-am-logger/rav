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
//! # Nothing builds one yet
//!
//! No [`Skin`] is constructed anywhere in rav. The terminal's shapes come from
//! `BarStyle`, an enum of the six styles `b` cycles, and it names its glyphs
//! directly rather than asking a skin for them.
//!
//! So this is the shape of the answer rather than the answer being used, and
//! the two differ in one way worth knowing: `BarStyle` carries the glyphs
//! themselves, while a [`Skin`] is meant to carry only how many steps there are
//! and let each surface decide what a step looks like. Wiring the terminal to
//! this is what would let one setting dress a glyph grid, a pixel surface and a
//! strip of LEDs at once - and it is why the artwork these describe is not here
//! either.

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

/// The shapes a bar is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Skin {
    pub name: &'static str,
    /// How many steps a rung divides into. The one thing the core is told.
    steps: usize,
    pub shapes: Shapes,
}

impl Skin {
    /// At least one step: a skin offering none could not draw a lit rung at all,
    /// and every division of a rung is by this number.
    pub const fn new(name: &'static str, steps: usize, shapes: Shapes) -> Self {
        Self {
            name,
            steps: if steps == 0 { 1 } else { steps },
            shapes,
        }
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
pub const SOLID: Skin = Skin::new("solid", 1, Shapes::Clipped);

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
        assert!(!SOLID.needs_a_shape_per_step());
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
