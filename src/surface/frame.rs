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
use rav_appearance::Skin;
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
    /// The ladder `b` chose, and how tall a rung is on this surface.
    ///
    /// A cell grid draws its own rungs with a glyph and wants `None`. A frame of
    /// pixels has no rungs of its own, so without this every bar style draws the
    /// same solid bar and `b` is six labels over one picture.
    pub ladder: Option<(Skin, Length)>,
}

/// Everything a frame needs, held for as long as it takes to draw.
pub struct Frame {
    bands: Vec<Band>,
    bars: Ramp,
    grid: Option<Ramp>,
    cap: Option<CapStyle>,
    ladder: Option<(Skin, Length)>,
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
            ladder: look.ladder,
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
            ladder: self.ladder,
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
            ladder: None,
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
            ladder: None,
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
        let clear = 3;
        let above_row = cap_row.checked_sub(clear);
        let below_row = cap_row + caps.get() as u32 + clear;
        // Stated rather than assumed. `at` indexes without checking, so a peak
        // nearer the ceiling than this would panic in the sampling rather than
        // fail on the thing being tested - and the failure would read as a bug
        // in the frame.
        let above_row =
            above_row.expect("the peak sits far enough from the ceiling to sample above it");
        assert!(
            below_row < TALL as u32,
            "the sample below the cap is off the bottom of the frame",
        );

        let above = at(&pixels, inside, above_row);
        let below = at(&pixels, inside, below_row);

        assert_ne!(
            above.3, 0,
            "the backdrop is missing above the cap - a notch, which is the \
             defect this surface exists to remove",
        );
        assert_ne!(below.3, 0, "the backdrop is missing below the cap");

        // And it is the backdrop either side, not more cap: the mark has to be
        // readable as a mark rather than smeared over the column. Near rather
        // than equal, because the backdrop is a ramp - a few pixels apart on it
        // is a shade apart, and demanding the same colour would be demanding the
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

    /// The same frame, drawn as the ladder `style` describes with 20px rungs.
    fn laddered(level: f32, style: crate::ui::analyzer::BarStyle) -> Vec<u8> {
        let theme = Theme::default();
        let palette = Palette::default();
        let look = Look {
            theme: &theme,
            palette: &palette,
            backdrop: false,
            caps: None,
            ladder: Some((style.skin(), Length(20.0))),
        };
        Frame::new(&[level], &[level], &look, layout(), screen())
            .pixels()
            .expect("a screen with area")
    }

    #[test]
    fn a_ladder_leaves_a_seam_where_its_glyph_does() {
        use crate::ui::analyzer::BarStyle;
        // What `b` chooses, drawn rather than named. `▇` is the lower seven
        // eighths of a cell, so a bar of it shows a gap in the top eighth of
        // every rung - which is the seam you see on a terminal, and which a
        // frame of pixels has to draw for itself.
        let inside = layout().column(0).left().get() as u32 + 5;
        // Rungs are 20 tall and the bar is full, so the floor of the second rung
        // up is 20 from the bottom and its top is 40.
        let second_rung_top = TALL as u32 - 40;

        let solid = laddered(1.0, BarStyle::Solid);
        // Seven eighths of 20 is 17.5, so the top two and a half pixels of the
        // rung are the seam. Two in is inside it; five in is under it.
        assert_eq!(
            at(&solid, inside, second_rung_top + 1).3,
            0,
            "no seam at the top of the rung - solid drew a solid bar",
        );
        assert_ne!(
            at(&solid, inside, second_rung_top + 5).3,
            0,
            "the rung itself is missing below its seam",
        );

        // Blocks fill the cell, so the same place is drawn.
        let blocks = laddered(1.0, BarStyle::Blocks);
        assert_ne!(
            at(&blocks, inside, second_rung_top + 1).3,
            0,
            "blocks left a seam, and a full block has none",
        );
    }

    #[test]
    fn a_rule_floats_and_a_ladder_does_not() {
        use crate::ui::analyzer::BarStyle;
        // `━` is a stroke across the middle of its cell, so a ladder of them is
        // rules with air above *and below* each one. Every other style sits on
        // the rung's floor.
        let inside = layout().column(0).left().get() as u32 + 5;
        let floor = TALL as u32 - 1;

        assert_eq!(
            at(&laddered(1.0, BarStyle::Line), inside, floor).3,
            0,
            "the rule is sitting on the floor rather than floating above it",
        );
        for style in [BarStyle::Blocks, BarStyle::Solid, BarStyle::Half] {
            assert_ne!(
                at(&laddered(1.0, style), inside, floor).3,
                0,
                "{style:?} left the floor of its bottom rung empty",
            );
        }
    }

    #[test]
    fn the_default_style_draws_exactly_what_it_drew_before_ladders() {
        // The acceptance property for giving `b` a meaning here: `blocks` is a
        // full cell, so its ladder tiles the bar and has to come out as the one
        // rectangle that was drawn before there were ladders at all. Byte for
        // byte - abutting antialiased edges could have left hairlines at every
        // rung boundary, and on the default style nobody would have called it a
        // regression, only "the bars look a bit odd now".
        use crate::ui::analyzer::BarStyle;
        let plain = capped(&[1.0], &[1.0], false, None);
        let theme = Theme::default();
        let palette = Palette::default();
        let look = Look {
            theme: &theme,
            palette: &palette,
            backdrop: false,
            caps: None,
            ladder: Some((BarStyle::Blocks.skin(), Length(20.0))),
        };
        let laddered = Frame::new(&[1.0], &[1.0], &look, layout(), screen())
            .pixels()
            .unwrap();
        assert_eq!(
            plain, laddered,
            "the blocks ladder is not the bar it replaced - look for hairlines \
             where the rungs meet",
        );
    }

    /// Why the pixel surface is expected to settle #63 and #64 rather than tune
    /// around them.
    #[test]
    fn a_fall_slides_on_pixels_where_it_steps_on_glyphs() {
        // A cell grid can only land a bar on an eighth of a row, so a slow fall
        // crosses a fixed number of visible steps - and how many depends on the
        // row count, which depends on the cell height, which differs by
        // platform: `DEFAULT_DPI` is 72 on macOS and 96 elsewhere. The same
        // descent stepping through more of them is what "the fall looks more
        // aggressive on Linux" would be made of.
        //
        // Pixels have no such ladder. Every level lands where it is.
        use crate::ui::scale::bar_eighths;
        use rav_core::geometry::{BarLayout, Screen};
        // 24 rows of 60 device pixels, which is what a WezTerm on this Mac
        // reports - not the toy screen the other tests use.
        let (rows, ch) = (24u16, 60.0f32);
        let screen = Screen::new(Length(WIDE), Length(f32::from(rows) * ch));
        let layout = BarLayout::new(Length(20.0), Length(4.0));
        let theme = Theme::default();
        let palette = Palette::default();
        let tall = (f32::from(rows) * ch) as u32;
        // Built once: nothing in the loop changes the look, only the level.
        let look = Look {
            theme: &theme,
            palette: &palette,
            backdrop: false,
            caps: None,
            ladder: None,
        };
        let inside = layout.column(0).left().get() as u32 + 5;
        let mut glyph_steps = std::collections::BTreeSet::new();
        let mut pixel_tops = std::collections::BTreeSet::new();
        for i in 0..40 {
            let level = 0.60 - i as f32 * 0.0015;
            glyph_steps.insert(bar_eighths(level, rows));
            let rgba = Frame::new(&[level], &[level], &look, layout, screen)
                .pixels()
                .expect("a screen with area");
            let alpha = |y: u32| rgba[((y * WIDE as u32 + inside) * 4 + 3) as usize];
            pixel_tops.insert((0..tall).find(|&y| alpha(y) != 0));
        }
        assert_eq!(
            pixel_tops.len(),
            40,
            "the pixel surface collapsed levels it should be able to express",
        );
        assert!(
            glyph_steps.len() * 2 < pixel_tops.len(),
            "the glyph ladder resolved {} of 40 levels and pixels {} - if these \
             are close, the cell is too short for this comparison to mean \
             anything and the numbers below need revisiting",
            glyph_steps.len(),
            pixel_tops.len(),
        );
    }

    #[test]
    fn a_shaded_ladder_is_the_same_colour_more_faintly() {
        use crate::ui::analyzer::BarStyle;
        // `▓` is a whole cell at three quarters - a density, not a height. The
        // bar still reddens as it rises, because the colour still comes from
        // the height; it is only fainter all the way up.
        let inside = layout().column(0).left().get() as u32 + 5;
        let low = TALL as u32 - 5;

        let solid = at(&laddered(1.0, BarStyle::Blocks), inside, low);
        let shaded = at(&laddered(1.0, BarStyle::Shade), inside, low);
        assert_ne!(solid.3, 0, "nothing to compare against");
        assert!(
            shaded.3 < solid.3,
            "the shade is not fainter: {shaded:?} against {solid:?}",
        );
        assert!(shaded.3 > 0, "the shade vanished entirely");
    }
}

#[cfg(test)]
mod against_the_glyphs {
    use super::*;
    use crate::ui::scale::bar_eighths;
    use rav_core::geometry::{BarLayout, Screen};

    #[test]
    fn a_pixel_bar_stands_where_the_glyph_bar_did_without_its_rounding() {
        // The acceptance test for moving off glyphs: the first pixel release has
        // to draw the picture the glyph release drew at the same size. Anything
        // else is a regression rather than the redesign it was sold as.
        //
        // Not identical, and it would be wrong if it were. A glyph bar is
        // truncated to a whole eighth of a cell - that is the finest a block
        // character goes - so the pixel bar is the same height or up to one
        // eighth taller, being the height nothing rounded. Never shorter, and
        // never more than the one eighth the rounding could account for.
        //
        // 80x24 cells at 30x60 device pixels, which is what a WezTerm on this
        // Mac reports, and one bar per cell column so the two line up.
        let (cols, rows, cw, ch) = (80u16, 24u16, 30.0f32, 60.0f32);
        let theme = Theme::default();
        let palette = Palette::default();
        let screen = Screen::new(Length(cols as f32 * cw), Length(rows as f32 * ch));
        let layout = BarLayout::new(Length(cw), Length(0.0));

        // A ramp across the display, reaching both ends: bar 0 is silence and the
        // last is full scale, which are the two heights most worth checking and
        // the two a ramp that stops short of them never sees.
        let levels: Vec<f32> = (0..cols as usize)
            .map(|i| i as f32 / (cols - 1) as f32)
            .collect();
        let look = Look {
            theme: &theme,
            palette: &palette,
            backdrop: false,
            caps: None,
            // The ladder production actually passes, not `None`. `blocks` is
            // what rav opens as, and testing the configuration nobody runs
            // would leave the one everybody runs unchecked.
            //
            // Only the styles whose ink reaches the top of their rung can be
            // compared this way: `line` is a rule at mid-height, so its topmost
            // ink sits half a rung below its bar top - on the glyph renderer
            // too, which draws the same stroke.
            ladder: Some((crate::ui::analyzer::BarStyle::Blocks.skin(), Length(ch))),
        };
        let rgba = Frame::new(&levels, &levels, &look, layout, screen)
            .pixels()
            .expect("a screen with area");

        let width = (cols as f32 * cw) as u32;
        let tall = (rows as f32 * ch) as u32;
        let drawn = |x: u32, y: u32| rgba[((y * width + x) * 4 + 3) as usize] != 0;

        for (i, &level) in levels.iter().enumerate() {
            let x = (i as f32 * cw + cw / 2.0) as u32;
            let top = (0..tall).find(|&y| drawn(x, y)).unwrap_or(tall);
            let in_eighths = ((tall - top) as f32 / ch * 8.0).round() as i64;
            let glyph = bar_eighths(level, rows) as i64;

            assert!(
                (in_eighths - glyph) == 0 || (in_eighths - glyph) == 1,
                "bar {i} at level {level:.3}: the glyph renderer draws {glyph} \
                 eighths and this draws {in_eighths} - the two have parted \
                 company by more than the eighth a block character rounds to",
            );
        }
    }
}
