//! Turning what the analyser measured into a frame of pixels.
//!
//! The join between the numbers and the picture, and the only place that knows
//! both. Above it the analyser deals in levels; below it the terminal deals in
//! bytes; here a theme becomes ramps and a set of levels becomes bars standing
//! in columns.
//!
//! Owned rather than borrowed, because a [`Scene`] holds references to its
//! bands and styles and something has to be holding them. That something lives
//! for a frame.

use crate::render::{Band, Canvas, CapStyle, Draw, Ramp, Scene, Style};
use crate::visual::{Palette, Theme};
use rav_core::geometry::{BarLayout, Screen};
use rav_core::units::{Length, Level};

/// How the picture should look, before any of it is resolved to colours.
///
/// A theme names colours and the user switches things on and off; neither is a
/// ramp until a palette has had its say, and only this layer has one. Keeping
/// them together is what the settings actually are - "the look" - rather than
/// four arguments that happen to travel side by side.
pub struct Look<'a> {
    pub theme: &'a Theme,
    /// The terminal's own colours, for a theme that named rather than spelled.
    pub palette: &'a Palette,
    /// The unlit part of each column, which `g` switches.
    pub backdrop: bool,
    /// How thick a peak cap is, or `None` when `p` has switched them off.
    pub caps: Option<Length>,
}

/// Everything a frame needs, held for as long as it takes to draw.
pub struct Frame {
    bands: Vec<Band>,
    bars: Ramp,
    grid: Option<Ramp>,
    cap: Option<CapStyle>,
    layout: BarLayout,
    screen: Screen,
}

impl Frame {
    /// Compose a frame from levels and the look they wear.
    ///
    /// `levels` and `peaks` are what the ballistics currently hold: a fraction
    /// of full scale per band.
    pub fn new(
        levels: &[f32],
        peaks: &[f32],
        look: &Look<'_>,
        layout: BarLayout,
        screen: Screen,
    ) -> Self {
        let bands = levels
            .iter()
            .zip(peaks.iter().chain(std::iter::repeat(&0.0)))
            .map(|(&level, &peak)| Band::new(Level::new(level), Level::new(peak)))
            .collect();
        Self {
            bands,
            bars: look.theme.bar_ramp(look.palette),
            grid: look.backdrop.then(|| look.theme.grid_ramp(look.palette)),
            cap: look.caps.map(|thickness| CapStyle {
                colour: look.theme.cap_colour(look.palette),
                thickness,
            }),
            layout,
            screen,
        }
    }

    /// Draw it, and hand back the pixels.
    ///
    /// RGBA, `width * height * 4` bytes, ready for the terminal to read - see
    /// [`super::pixels`], which never encodes them.
    pub fn pixels(&self) -> Option<Vec<u8>> {
        let mut canvas = Canvas::for_screen(&self.screen)?;
        canvas.clear();
        let styles = [Style {
            bars: &self.bars,
            grid: self.grid.as_ref(),
            cap: self.cap,
        }];
        Scene {
            bands: &self.bands,
            layout: self.layout,
            screen: self.screen,
            styles: &styles,
        }
        .draw(&mut canvas);
        Some(canvas.to_rgba())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rav_core::geometry::Column;

    const WIDE: f32 = 240.0;
    const TALL: f32 = 120.0;

    fn screen() -> Screen {
        Screen::new(Length(WIDE), Length(TALL))
    }

    fn layout() -> BarLayout {
        BarLayout::new(Length(20.0), Length(4.0))
    }

    /// The pixel at `(x, y)`, as `(r, g, b, a)`.
    fn at(pixels: &[u8], x: u32, y: u32) -> (u8, u8, u8, u8) {
        let i = ((y * WIDE as u32 + x) * 4) as usize;
        (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3])
    }

    /// The default look, for the cases that are not about how it looks.
    ///
    /// Leaks its theme and palette rather than threading two more bindings
    /// through every caller; a handful of allocations for the whole test run.
    fn plain() -> Look<'static> {
        Look {
            theme: Box::leak(Box::new(Theme::default())),
            palette: Box::leak(Box::new(Palette::default())),
            backdrop: true,
            caps: None,
        }
    }

    fn frame(levels: &[f32], peaks: &[f32], backdrop: bool) -> Vec<u8> {
        capped(levels, peaks, backdrop, Some(Length(2.0)))
    }

    fn capped(levels: &[f32], peaks: &[f32], backdrop: bool, caps: Option<Length>) -> Vec<u8> {
        let theme = Theme::default();
        let palette = Palette::default();
        let look = Look {
            theme: &theme,
            palette: &palette,
            backdrop,
            caps,
        };
        Frame::new(levels, peaks, &look, layout(), screen())
            .pixels()
            .expect("a screen with area")
    }

    #[test]
    fn a_frame_is_the_size_the_screen_asked_for() {
        let pixels = frame(&[0.5], &[0.5], true);
        assert_eq!(pixels.len(), (WIDE * TALL * 4.0) as usize);
    }

    #[test]
    fn a_full_bar_wears_the_top_of_the_ramp_and_a_silent_one_the_bottom() {
        // The rule the whole look rests on: colour follows height, not the
        // band's own level. A bar reaching the ceiling is red up there and green
        // down at the floor - the same green a bar that never leaves the floor
        // shows.
        let pixels = frame(&[1.0], &[1.0], false);
        let inside = layout().column(0).left().get() as u32 + 5;

        let floor = at(&pixels, inside, TALL as u32 - 2);
        let ceiling = at(&pixels, inside, 4);
        assert!(
            floor.1 > floor.0,
            "the foot of the bar is not green: {floor:?}"
        );
        assert!(ceiling.0 > ceiling.1, "the top is not red: {ceiling:?}");
    }

    #[test]
    fn the_backdrop_shows_the_colour_a_bar_would_reach() {
        // What `g` turns on: the unlit part of a column previewing where the bar
        // is going. Above a half-height bar the column is still painted, and in
        // the same hue family the bar would be if it got there.
        let lit = frame(&[0.5], &[0.5], true);
        let bare = frame(&[0.5], &[0.5], false);
        let inside = layout().column(0).left().get() as u32 + 5;

        let above_the_bar = TALL as u32 / 4;
        assert_ne!(
            at(&lit, inside, above_the_bar).3,
            0,
            "the backdrop left the column empty",
        );
        assert_eq!(
            at(&bare, inside, above_the_bar).3,
            0,
            "the column was painted with the backdrop switched off",
        );
    }

    #[test]
    fn a_silent_band_shows_its_cap_resting_on_the_floor_and_nothing_else() {
        // Every frame of a quiet passage. With caps off a silent band is empty;
        // with them on the only thing drawn is the cap, held inside the screen
        // at the bottom rather than half off it.
        let inside = layout().column(0).left().get() as u32 + 5;

        let bare = capped(&[0.0], &[0.0], false, None);
        for y in [1u32, TALL as u32 / 2, TALL as u32 - 2] {
            assert_eq!(at(&bare, inside, y).3, 0, "something drawn at y={y}");
        }

        let with_cap = capped(&[0.0], &[0.0], false, Some(Length(2.0)));
        assert_eq!(at(&with_cap, inside, 1).3, 0, "something up at the ceiling");
        assert_eq!(
            at(&with_cap, inside, TALL as u32 / 2).3,
            0,
            "a bar where there is no signal",
        );
        assert_ne!(
            at(&with_cap, inside, TALL as u32 - 1).3,
            0,
            "the cap fell off the bottom of the screen",
        );
    }

    #[test]
    fn the_gap_between_columns_stays_empty() {
        // Bars that merged into a solid block would read as one loud band across
        // the whole spectrum.
        let pixels = frame(&[1.0, 1.0], &[1.0, 1.0], true);
        let first = layout().column(0);
        let second = layout().column(1);
        let between = (first.right().get() + second.left().get()) as u32 / 2;
        assert_eq!(
            at(&pixels, between, TALL as u32 / 2).3,
            0,
            "the columns ran together",
        );
    }

    #[test]
    fn a_cap_is_drawn_where_the_bar_is_not() {
        // The defect a terminal cell cannot fix: there, a cap and the backdrop
        // compete for one cell and the cap wins by erasing it. Here they are two
        // rectangles, so the cap marks the peak *above* a lower bar without
        // taking anything away from it.
        let pixels = frame(&[0.3], &[0.8], true);
        let inside = layout().column(0).left().get() as u32 + 5;

        // The cap rides at 0.8 of the height, measured down from the ceiling.
        let cap_row = (TALL * (1.0 - 0.8)) as u32;
        let cap = at(&pixels, inside, cap_row);
        let bar = at(&pixels, inside, TALL as u32 - 4);
        assert_ne!(cap.3, 0, "no cap at the peak");
        assert_ne!(bar.3, 0, "the bar went missing under its own cap");
        assert_ne!(cap, bar, "the cap is indistinguishable from the bar");
    }

    #[test]
    fn the_backdrop_runs_right_up_to_the_cap_and_out_the_other_side() {
        // This is the whole argument for owning the pixels, stated where it can
        // be checked: issue #65's third defect, which no terminal build can fix.
        //
        // A cell holds one symbol. When a cap crosses the backdrop there, the
        // cap's glyph *replaces* the backdrop's, so a notch a whole cell tall
        // travels down the column behind the falling cap. Measured on the glyph
        // renderer: with the grid on and a bar style whose backdrop is a glyph -
        // solid, thick, half, line, four of the six - the backdrop rung at the
        // cap's row is simply gone.
        //
        // Here the cap is two pixels of its own, so the backdrop is intact one
        // pixel above it and one pixel below. That is the difference between
        // losing a cell of backdrop and losing nothing.
        let caps = Length(2.0);
        let pixels = capped(&[0.3], &[0.8], true, Some(caps));
        let inside = layout().column(0).left().get() as u32 + 5;
        let cap_row = (TALL * (1.0 - 0.8)) as u32;

        let cap = at(&pixels, inside, cap_row);
        assert_ne!(cap.3, 0, "no cap at the peak");

        // Far enough from the cap to be past its thickness, close enough that a
        // terminal drawing this would still be inside the one cell it lost.
        let above = at(&pixels, inside, cap_row - 3);
        let below = at(&pixels, inside, cap_row + caps.get() as u32 + 3);

        assert_ne!(
            above.3, 0,
            "the backdrop is missing above the cap - a notch, which is the \
             defect this surface exists to remove",
        );
        assert_ne!(below.3, 0, "the backdrop is missing below the cap");

        // And it is the backdrop either side, not more cap: the mark has to be
        // readable as a mark rather than smeared over the column. Near rather
        // than equal, because the backdrop is a ramp - six pixels apart on it is
        // a shade apart, and demanding the same colour would be demanding the
        // ramp not be a ramp.
        let apart = |a: (u8, u8, u8, u8), b: (u8, u8, u8, u8)| {
            u32::from(a.0.abs_diff(b.0))
                + u32::from(a.1.abs_diff(b.1))
                + u32::from(a.2.abs_diff(b.2))
        };
        assert!(
            apart(above, below) < 16,
            "the backdrop is a different colour either side of the cap: \
             {above:?} against {below:?}",
        );
        assert!(
            apart(above, cap) > 64,
            "the cap is indistinguishable from the backdrop: {cap:?} against \
             {above:?}",
        );
    }

    #[test]
    fn a_screen_with_no_area_gives_no_frame() {
        // A terminal mid-resize, every time one is dragged.
        let nothing = Screen::new(Length::NONE, Length::NONE);
        let none = Frame::new(&[0.5], &[0.5], &plain(), layout(), nothing);
        assert!(none.pixels().is_none());
    }

    #[test]
    fn fewer_peaks_than_bands_is_not_a_panic() {
        // The two arrive from the same place today, but a resize lands between
        // them often enough that a mismatch must be a quiet frame rather than a
        // crash mid-render.
        let short = Frame::new(&[0.5, 0.5, 0.5], &[0.5], &plain(), layout(), screen());
        assert!(short.pixels().is_some());
    }

    /// Where a column sits, for the assertions above.
    #[test]
    fn the_columns_are_where_the_tests_think_they_are() {
        let first: Column = layout().column(0);
        assert_eq!(first.left(), Length::NONE);
        assert_eq!(first.right(), Length(20.0));
        assert_eq!(layout().column(1).left(), Length(24.0));
    }
}
