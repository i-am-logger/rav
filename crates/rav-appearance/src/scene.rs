//! What a surface is asked to draw.
//!
//! The seam between the analyser and whatever is drawing. Deliberately not a
//! retained scene graph: a glyph renderer has to quantise to cells with its own
//! rules - the fill-versus-glyph backdrop, cap lifting, partial blocks - and
//! quantising a shared list of rectangles back into cells cannot reproduce them.
//! Every surface takes the same scene and each is free to be itself.
//!
//! Nothing here can draw. A scene that knew how to rasterise itself would be a
//! scene only a rasteriser could consume, and the LED matrix has none.

use crate::ink::Colour;
use crate::ramp::Ramp;
use rav_core::geometry::{BarLayout, Screen};
use rav_core::units::{Length, Level};

/// Which of a scene's styles a band wears.
///
/// An index, not a colour - the same boundary [`crate::units::Step`] keeps. A
/// band knows it wears style 1; only the surface knows style 1 is a blue ramp
/// with a white cap.
///
/// Everything is style zero unless something says otherwise, so the common case
/// costs one byte per band and no thought. A stereo pair is `0`/`1`, a rainbow
/// is one per band, and highlighting a single band is one `1` among zeroes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct StyleId(pub u8);

impl StyleId {
    pub const FIRST: Self = Self(0);

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One frequency band, as the analyser currently sees it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Band {
    /// How loud it is now.
    pub level: Level,
    /// How loud it recently was - where the cap rides.
    pub peak: Level,
    /// Which style it wears.
    pub style: StyleId,
}

impl Band {
    /// A band in the first style, which is what almost every band is.
    pub fn new(level: Level, peak: Level) -> Self {
        Self {
            level,
            peak,
            style: StyleId::FIRST,
        }
    }

    /// The same band wearing a different style - one channel of a stereo pair,
    /// or a highlighted band.
    pub fn styled(self, style: StyleId) -> Self {
        Self { style, ..self }
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
    /// The looks a band may wear, indexed by its [`StyleId`].
    ///
    /// Usually one. Two for a stereo pair, one per band for a rainbow. A scene
    /// with none draws nothing rather than inventing a colour.
    pub styles: &'a [Style<'a>],
}

/// One complete look: what a lit bar, its backdrop and its cap are drawn in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style<'a> {
    /// Colour of a lit bar, by height.
    pub bars: &'a Ramp,
    /// Colour of the unlit backdrop, by height. `None` leaves the terminal's own
    /// background showing, which is what a theme without a grid asks for.
    pub grid: Option<&'a Ramp>,
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
    /// the screen, which a rasteriser discards in silence - so the surfaces
    /// would disagree on bar count with nothing to say so.
    pub fn visible_bands(&self) -> usize {
        self.layout
            .fitting_across(&self.screen)
            .min(self.bands.len())
    }

    /// The style a band wears, or the first if it names one that is not there.
    ///
    /// Falling back rather than panicking: a band carrying a stale style after a
    /// theme change is a frame drawn in the wrong colour, not a reason to take
    /// the process down mid-render.
    pub fn style_of(&self, band: &Band) -> Option<&Style<'_>> {
        self.styles
            .get(band.style.index())
            .or_else(|| self.styles.first())
    }
}
