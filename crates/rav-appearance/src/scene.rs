//! What a surface is asked to draw.
//!
//! The seam between the analyser and whatever is drawing. Deliberately not a
//! retained scene graph: a glyph renderer has to quantise to cells with its own
//! rules - the fill-versus-glyph backdrop, cap lifting, partial blocks - and
//! quantising a shared list of rectangles back into cells cannot reproduce them.
//! A surface takes the scene and is then free to be itself.
//!
//! Nothing here can draw. A scene that knew how to rasterise itself would be a
//! scene only a rasteriser could consume, and the LED matrix has none.
//!
//! # Taken by one surface
//!
//! The pixel surface, on every frame it draws - which is every frame on a
//! terminal that says it can draw images. The glyph renderer takes levels and
//! colours directly and quantises them itself, so this is the seam for one
//! consumer until a second is built on it: a window or an LED matrix would be
//! the second, and neither exists yet.

use crate::ink::Colour;
use crate::ramp::Ramp;
use crate::skin::Skin;
use rav_core::geometry::{BarLayout, Screen};
use rav_core::transform::Transform;
use rav_core::units::{Length, Level};

/// Which of a scene's styles a band wears.
///
/// An index, not a colour - the same boundary [`rav_core::units::Step`] keeps. A
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

/// Where the viewer is standing.
///
/// A whole-field property, not a per-bar one: the analyser is a panel, and this
/// is the angle it is bolted at. Named rather than handed over as a matrix so
/// that a surface cannot get the centring wrong - a perspective shrinks toward
/// the origin, and the middle of the picture is the only sensible place for it
/// to shrink toward, which only the scene knows.
///
/// A surface that cannot honour one draws [`Flat`](Self::Flat) and is right to:
/// a cell grid has no room for a bar that leans, and half a lean is worse than
/// none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Straight on, which is what every surface has always drawn.
    #[default]
    Flat,
    /// Tipped back, so the top of the field falls away - a dashboard raked
    /// back from the driver.
    Raked,
    /// Turned to one side - a panel on a speaker grille, seen from off-axis.
    Turned,
    /// Standing back from the bass and away toward the treble, so the spectrum
    /// reads as a corridor rather than a fence.
    ///
    /// The only view where the bars are at different depths from each other
    /// rather than the whole field being at an angle - so it is the only one
    /// that changes what order they have to be drawn in.
    Corridor,
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Raked => "raked",
            Self::Turned => "turned",
            Self::Corridor => "corridor",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Flat => Self::Raked,
            Self::Raked => Self::Turned,
            Self::Turned => Self::Corridor,
            Self::Corridor => Self::Flat,
        }
    }

    /// Whether the bands stand at different depths from one another.
    ///
    /// Which decides the order they are drawn in, and nothing else: near over
    /// far is the whole of what a painter has instead of a depth buffer, and
    /// getting it backwards puts the quiet distance on top of the loud
    /// foreground.
    pub fn recedes(self) -> bool {
        matches!(self, Self::Corridor)
    }

    /// How far back the band at `index` of `bands` stands.
    ///
    /// Bass near, treble far, which is the way round a spectrum is already
    /// read - and the way round that puts the bands people watch closest to
    /// them. Zero for every view that keeps the field on one plane, so those
    /// compose to exactly the transform they had before this existed.
    pub fn depth(self, index: usize, bands: usize, screen: &Screen) -> f32 {
        if !self.recedes() || bands < 2 {
            return 0.0;
        }
        let along = index.min(bands - 1) as f32 / (bands - 1) as f32;
        // Two thirds of the way to the eye at the far end: enough for the
        // treble to read as distant, short of the vanishing point where a bar
        // is a few pixels and its colour is the only thing left of it.
        along * screen.width().get() * 2.0
    }

    /// The transform this comes to on a screen of a given size.
    ///
    /// Turn about the middle, then look at it from in front of the middle:
    /// `move to the origin, rotate, recede, move back`. The move back survives
    /// the perspective divide because it multiplies the `w` the projection just
    /// set, which is what puts the picture in the middle of the screen rather
    /// than in the middle of a screen that has itself been shrunk.
    ///
    /// The eye stands well back - three times the size of the field - so the
    /// nearest corner never reaches it. A rake steep enough to swing a corner
    /// behind the viewer would draw nothing at all, which reads as a fault
    /// rather than as an angle.
    ///
    /// **And then it is scaled to fit.** An angle changes the shape of the
    /// field, never how much of the screen it takes: a perspective magnifies
    /// whatever it brings nearer, so a raked field spills off three edges and
    /// the bars run out of the bottom of the picture. That is a rule rather
    /// than a number chosen by looking - the fit is computed from the corners
    /// the rotation actually produced, so the angles can be whatever reads best
    /// and none of them can push the picture off the screen.
    pub fn placing(self, screen: &Screen) -> Transform {
        let (across, down) = (screen.width().get(), screen.height().get());
        let (middle_x, middle_y) = (across / 2.0, down / 2.0);
        let (turn, eye) = match self {
            Self::Flat => return Transform::IDENTITY,
            Self::Raked => (Transform::leaning(0.35), down * 3.0),
            Self::Turned => (Transform::turning(0.44), across * 3.0),
            // No rotation: the field stays square on and the bands travel back
            // through it. Everything on the near plane is left exactly where it
            // was, so the fit below finds nothing to do and the picture keeps
            // the whole screen.
            Self::Corridor => (Transform::IDENTITY, across * 3.0),
        };
        let centred = |about: Transform| {
            Transform::moving(-middle_x, -middle_y, 0.0)
                .then(about)
                .then(Transform::moving(middle_x, middle_y, 0.0))
        };
        let angled = Transform::moving(-middle_x, -middle_y, 0.0)
            .then(turn)
            .then(Transform::receding(eye))
            .then(Transform::moving(middle_x, middle_y, 0.0));

        let Some(field) = angled.quad(screen.bounds()) else {
            return angled;
        };
        let (mut wide, mut tall) = (0.0f32, 0.0f32);
        for corner in field.corners {
            wide = wide.max((corner.x.get() - middle_x).abs() * 2.0);
            tall = tall.max((corner.y.get() - middle_y).abs() * 2.0);
        }
        let fits = (across / wide).min(down / tall);
        if !fits.is_finite() || fits >= 1.0 {
            return angled;
        }
        angled.then(centred(Transform::scaling(fits, fits, 1.0)))
    }
}

/// Everything a surface needs to draw one frame.
///
/// Borrowed throughout, and built fresh each frame rather than kept: a scene is
/// what the analyser currently reads, so holding one across frames would be
/// holding a picture of the past. See the module note for why it is a flat list
/// rather than a graph, and for which surfaces take it.
pub struct Scene<'a> {
    pub bands: &'a [Band],
    pub layout: BarLayout,
    pub screen: Screen,
    /// The angle the whole field is bolted at. `Flat` is what every surface
    /// has always drawn, and what one that cannot lean draws regardless.
    pub view: View,
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
    /// The ladder a bar is built from, and how tall one of its rungs is here.
    ///
    /// `None` draws the bar as one shape from the floor, which is what a
    /// surface with rungs of its own wants: a cell grid already draws the rung
    /// with a glyph, and drawing it twice would fight the glyph. A surface with
    /// no rungs of its own draws them from this, which is the only way `b`
    /// means anything on it.
    pub ladder: Option<(Skin, Length)>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::Colour;
    use alloc::vec;
    use alloc::vec::Vec;

    fn ramp(colour: Colour) -> Ramp {
        Ramp::new(vec![colour])
    }

    fn scene<'a>(bands: &'a [Band], styles: &'a [Style<'a>], across: f32) -> Scene<'a> {
        Scene {
            bands,
            layout: BarLayout::new(Length(8.0), Length(2.0)),
            screen: Screen::new(Length(across), Length(100.0)),
            view: View::Flat,
            styles,
        }
    }

    #[test]
    fn flat_is_the_view_that_asks_for_nothing() {
        // Every surface has always drawn this, and one that cannot lean draws it
        // regardless - so it has to come out as the transform that means "leave
        // it alone", exactly, or the pixel surface stops taking the path it has
        // taken since before transforms existed.
        let screen = Screen::new(Length(240.0), Length(120.0));
        assert!(View::default() == View::Flat);
        assert!(View::Flat.placing(&screen).is_identity());
        assert!(!View::Raked.placing(&screen).is_identity());
        assert!(!View::Turned.placing(&screen).is_identity());
    }

    #[test]
    fn the_middle_of_the_picture_stays_where_it_is() {
        // A perspective shrinks toward the origin, and the origin is the top-left
        // corner. Without turning about the middle and coming back, every angle
        // would also slide the whole field up and to the left - which reads as a
        // bug rather than as a viewing angle, and is why this is named rather
        // than handed to a surface as a matrix to centre for itself.
        let screen = Screen::new(Length(240.0), Length(120.0));
        let middle = rav_core::transform::Point::flat(Length(120.0), Length(60.0));
        for view in [View::Raked, View::Turned, View::Corridor] {
            let stays = view.placing(&screen).apply(middle).unwrap();
            assert!(
                (stays.x.get() - 120.0).abs() < 0.01 && (stays.y.get() - 60.0).abs() < 0.01,
                "{} moved the middle to {stays:?}",
                view.label(),
            );
        }
    }

    #[test]
    fn every_angle_keeps_the_whole_field_in_front_of_the_viewer() {
        // A corner swung behind the eye draws nothing, which reads as a fault
        // rather than as an angle - so the eye stands well back and every corner
        // of every offered view has somewhere to land.
        let screen = Screen::new(Length(240.0), Length(120.0));
        let field = screen.bounds();
        for view in [View::Flat, View::Raked, View::Turned, View::Corridor] {
            assert!(
                view.placing(&screen).quad(field).is_some(),
                "{} put a corner where it cannot be drawn",
                view.label(),
            );
        }
    }

    #[test]
    fn an_angle_changes_the_shape_of_the_field_not_how_much_screen_it_takes() {
        // A perspective magnifies whatever it brings nearer, so a rake with
        // nothing holding it fans the near edge out past three sides and the
        // bars run off the bottom of the picture - which was measured by
        // rendering one and looking at it. The fit is computed from the corners
        // the rotation produced rather than chosen by eye, so the angles can be
        // whatever reads best and none of them can spill.
        let screen = Screen::new(Length(480.0), Length(240.0));
        let field = screen.bounds();
        for view in [View::Flat, View::Raked, View::Turned, View::Corridor] {
            let quad = view.placing(&screen).quad(field).expect("undrawable");
            for corner in quad.corners {
                let (x, y) = (corner.x.get(), corner.y.get());
                assert!(
                    (-0.01..=480.01).contains(&x) && (-0.01..=240.01).contains(&y),
                    "{} put a corner at ({x}, {y}), off a 480x240 screen",
                    view.label(),
                );
            }
        }
    }

    #[test]
    fn the_angles_cycle_back_to_flat() {
        // Three presses and the picture is where it started, like every other
        // cycling setting in rav.
        let mut view = View::Flat;
        for expected in [View::Raked, View::Turned, View::Corridor, View::Flat] {
            view = view.next();
            assert_eq!(view, expected);
        }
    }

    #[test]
    fn a_scene_draws_the_lesser_of_what_fits_and_what_exists() {
        // The glyph renderer's rule, and the reason it is here rather than in
        // each surface: one per band regardless puts rectangles past the screen,
        // where a rasteriser drops them without a word - so two surfaces would
        // disagree about how many bars there are and neither would say so.
        let green = ramp(Colour::GREEN);
        let styles = [Style {
            bars: &green,
            grid: None,
            cap: None,
            ladder: None,
        }];
        let many: Vec<Band> = (0..64).map(|_| Band::default()).collect();

        // 8 wide with a 2 gap is a stride of 10, so 5 fit across 48.
        assert_eq!(scene(&many, &styles, 48.0).visible_bands(), 5, "clipped");
        let few = [Band::default(), Band::default()];
        assert_eq!(
            scene(&few, &styles, 480.0).visible_bands(),
            2,
            "all of them"
        );
    }

    #[test]
    fn a_stereo_pair_is_two_styles_and_one_scene() {
        // What "a mode can have a different colour per channel" comes to. The
        // bands carry an index and the surface holds the looks, so a mirrored
        // pair needs no second scene and no second analyser.
        let left = ramp(Colour::CYAN);
        let right = ramp(Colour::MAGENTA);
        let styles = [
            Style {
                bars: &left,
                grid: None,
                cap: None,
                ladder: None,
            },
            Style {
                bars: &right,
                grid: None,
                cap: None,
                ladder: None,
            },
        ];
        let bands = [
            Band::new(Level::FULL, Level::FULL),
            Band::new(Level::FULL, Level::FULL).styled(StyleId(1)),
        ];
        let scene = scene(&bands, &styles, 480.0);
        assert_eq!(
            scene.style_of(&bands[0]).unwrap().bars.stops()[0],
            Colour::CYAN
        );
        assert_eq!(
            scene.style_of(&bands[1]).unwrap().bars.stops()[0],
            Colour::MAGENTA,
        );
    }

    #[test]
    fn a_band_wearing_a_style_that_is_gone_is_drawn_in_the_first_one() {
        // Reachable: a theme change swaps the styles while bands from the
        // previous frame still name the old ones. A wrong colour for one frame
        // is a blink; a panic mid-render loses the terminal the user is in.
        let green = ramp(Colour::GREEN);
        let styles = [Style {
            bars: &green,
            grid: None,
            cap: None,
            ladder: None,
        }];
        let stale = Band::default().styled(StyleId(9));
        let bands = [stale];
        let scene = scene(&bands, &styles, 480.0);
        assert_eq!(
            scene.style_of(&stale).unwrap().bars.stops()[0],
            Colour::GREEN,
        );
    }

    #[test]
    fn a_scene_with_no_styles_draws_nothing_rather_than_inventing_a_colour() {
        // The alternative is a default, and a default here is rav choosing a
        // colour the theme did not - which is the one thing the appearance
        // layer exists to prevent.
        let band = Band::default();
        let bands = [band];
        assert!(scene(&bands, &[], 480.0).style_of(&band).is_none());
    }

    #[test]
    fn everything_is_the_first_style_until_something_says_otherwise() {
        // So the common case costs one byte per band and no thought.
        assert_eq!(Band::default().style, StyleId::FIRST);
        assert_eq!(Band::new(Level::FULL, Level::FULL).style, StyleId::FIRST);
        assert_eq!(StyleId::FIRST.index(), 0);
    }
}
