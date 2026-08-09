//! The analyser view: bars, peak caps and the dotted background behind them.

use crate::ui::scale::{bar_eighths, cap_position, ramp_index, segment_top, CAP_LOW, CAP_MID};
use crate::visual::{Skin, Theme};
use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

/// Eighth-block glyphs, empty through full.
const BLOCKS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// What a lit bar is drawn with.
///
/// `Blocks` is the default: eight steps within a cell is most of the vertical
/// resolution a terminal has, and it is the closest to the panel this look comes
/// from.
///
/// Two of them resolve below a whole row, by different means. `Blocks` climbs
/// `▁▂▃▄▅▆▇` - the cell grows upward - which is eight steps and the smoothest.
/// `Shade` has no such ladder, because Unicode has no shaded eighth-blocks: the
/// shades are three fixed densities of a *whole* cell. So it fills the top cell
/// in density instead, `░▒▓`, and the bar fades in rather than rising.
///
/// The rest light whole rows and differ only in how much of a cell they fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// `▓`, with `░▒` for a top cell that is partly there.
    Shade,
    /// `█` with `▁`..`▇` for the partial top cell. The original's look, smoothest.
    #[default]
    Blocks,
    /// `▇` - lower seven-eighths; densest that still shows a seam.
    Solid,
    /// `▆` - lower three-quarters; near-solid with a thin dark separator.
    Thick,
    /// `▄` - lower half; visibly striped.
    Half,
    /// `━` - a thin rule, the airiest ladder.
    Line,
}

impl BarStyle {
    pub fn next(self) -> Self {
        match self {
            BarStyle::Shade => BarStyle::Blocks,
            BarStyle::Blocks => BarStyle::Solid,
            BarStyle::Solid => BarStyle::Thick,
            BarStyle::Thick => BarStyle::Half,
            BarStyle::Half => BarStyle::Line,
            BarStyle::Line => BarStyle::Shade,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BarStyle::Shade => "shade",
            BarStyle::Blocks => "blocks",
            BarStyle::Solid => "solid",
            BarStyle::Thick => "thick",
            BarStyle::Half => "half",
            BarStyle::Line => "line",
        }
    }

    /// Glyph for a fully lit row.
    fn glyph(self) -> &'static str {
        match self {
            BarStyle::Shade => "\u{2593}",
            BarStyle::Blocks => BLOCKS[8],
            BarStyle::Solid => "\u{2587}",
            BarStyle::Thick => "\u{2586}",
            BarStyle::Half => "\u{2584}",
            BarStyle::Line => "\u{2501}",
        }
    }

    /// Whether the style can show a row that is only partly filled.
    fn sub_cell(self) -> bool {
        matches!(self, BarStyle::Blocks | BarStyle::Shade)
    }

    /// Glyph for a top cell that is `remainder` eighths of the way up.
    fn partial(self, remainder: usize) -> &'static str {
        match self {
            BarStyle::Blocks => BLOCKS[remainder],
            // Density rather than height. Unicode has no shaded eighth-blocks -
            // the shades are three fixed densities of a *whole* cell - so the top
            // of a shade bar fades in rather than rising. Three steps, not eight.
            BarStyle::Shade => match remainder {
                0 => " ",
                1 => "░",
                2..=4 => "▒",
                _ => "▓",
            },
            // The rest light whole rows and never report a remainder.
            _ => self.glyph(),
        }
    }

    /// Whether a lit row covers its whole cell.
    ///
    /// This is what decides how the backdrop is drawn. `Shade` counts: `▓` is a
    /// full cell with a texture, not a partial one.
    fn fills_cell(self) -> bool {
        matches!(self, BarStyle::Shade | BarStyle::Blocks)
    }

    /// Glyph the unlit grid behind the bars is drawn with.
    ///
    /// A plain full block for the full-cell styles rather than the shade: the
    /// backdrop is a backdrop, and the texture belongs to the lit bar in front of
    /// it. The partial styles keep their own glyph, so the grid has the same
    /// shape as the bars it sits behind.
    fn grid_glyph(self) -> &'static str {
        if self.fills_cell() {
            BLOCKS[8]
        } else {
            self.glyph()
        }
    }
}

/// How finely the peak cap is placed within a row.
///
/// The cap falls slowly and its exact height is the thing you watch, so this is
/// where vertical resolution is worth spending. Higher levels subdivide the row
/// using block glyphs; the trade-off is weight, since Unicode has no middle
/// one-eighth block and the mid-cell position must come from a box-drawing
/// stroke the font sizes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Peaks {
    Off,
    /// One position per row: the mid-cell rule, as the original draws it. Evenly
    /// spaced, but the cap visibly steps as it falls.
    Coarse,
    /// Two positions per row: the mid-cell rule, then the lower one-eighth
    /// block. The default - the cap sits mid-cell and then drops to the bar's
    /// edge, which is smoother than stepping a whole row at a time.
    #[default]
    Fine,
}

impl Peaks {
    pub fn next(self) -> Self {
        match self {
            Peaks::Fine => Peaks::Coarse,
            Peaks::Coarse => Peaks::Off,
            Peaks::Off => Peaks::Fine,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Peaks::Off => "off",
            Peaks::Coarse => "coarse",
            Peaks::Fine => "fine",
        }
    }

    /// Row and glyph for a cap at `peak`, or `None` when caps are off.
    fn cell(self, peak: f32, height: u16) -> Option<(u16, &'static str)> {
        if self == Peaks::Off {
            return None;
        }
        let (row, frac) = cap_position(peak, height);
        // The upper one-eighth block is deliberately unused. It puts the mark at
        // the very top of its cell, which on the display's top row presses it
        // against the screen edge, and everywhere else leaves a visible gap
        // between the cap and the bar below it. Mid-cell then bottom-of-cell
        // descends just as smoothly without ever detaching.
        let glyph = match self {
            Peaks::Off => unreachable!(),
            Peaks::Coarse => CAP_MID,
            Peaks::Fine => {
                if frac < 0.5 {
                    CAP_LOW
                } else {
                    CAP_MID
                }
            }
        };
        Some((row, glyph))
    }
}

/// Bar geometry. The original draws 3px bars with a 1px gap in "wide" mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarLayout {
    pub bar_width: u16,
    pub spacing: u16,
}

impl Default for BarLayout {
    fn default() -> Self {
        Self {
            bar_width: 3,
            spacing: 1,
        }
    }
}

impl BarLayout {
    /// How many bars fit in `width`. The final bar needs no trailing gap.
    pub fn bar_count(&self, width: u16) -> usize {
        let slot = (self.bar_width + self.spacing).max(1);
        ((width + self.spacing) / slot) as usize
    }

    /// Left edge of bar `i`, relative to the area.
    pub fn x_of(&self, i: usize) -> u16 {
        i as u16 * (self.bar_width + self.spacing)
    }
}

/// Renders one frame of the analyser. Borrows everything, so constructing it is
/// free and it can be built fresh inside the draw closure.
pub struct Analyzer<'a> {
    pub bars: &'a [f32],
    pub peaks: &'a [f32],
    /// Foreground colour per screen row, bottom-up. Cached by the caller.
    pub row_colors: &'a [Color],
    pub cap_color: Color,
    /// Backdrop colour per screen row, bottom-up, or `None` to hide it. Cached
    /// by the caller alongside `row_colors`.
    pub grid: Option<&'a [Color]>,
    pub bar_style: BarStyle,
    pub layout: BarLayout,
    pub peaks_style: Peaks,
}

impl Widget for Analyzer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || self.row_colors.is_empty() {
            return;
        }
        let height = area.height;
        // ratatui re-queries the terminal size inside `draw`, so a resize landing
        // between the caller measuring and this running leaves `row_colors`
        // shorter than the area. Indexing it directly panicked, and the unwind
        // skipped the raw-mode teardown, leaving the terminal unusable.
        let row_color =
            |r: u16| -> Color { self.row_colors[(r as usize).min(self.row_colors.len() - 1)] };

        // The terminal's own background is left alone deliberately. The
        // original painted its own across the whole panel, but a terminal
        // visualiser should sit in whatever theme the user already has.
        //
        // Every bar gets a dim unlit twin behind it, aligned to the bar columns
        // and drawn first so the lit bars paint over it.
        //
        // For a full-cell style the grid is a *background fill*, not a block
        // glyph. A block glyph leaves a hairline gap between cells in most
        // terminal fonts, so a cell that is filled instead - the cap's, or a
        // partial top - is visibly more solid than its neighbours, and that patch
        // slides down the column as the cap falls. Filling every cell makes them
        // all tile identically, and anything drawn on top only sets a symbol and
        // a foreground.
        //
        // The partial styles keep their own glyph and no fill: a solid backdrop
        // behind a ladder of thin rules would be a slab, and the same odd-cell-out
        // problem would appear in reverse.
        //
        let fill = self.bar_style.fills_cell();
        if let Some(grid) = self.grid {
            let grid_color = |r: u16| -> Color { grid[(r as usize).min(grid.len() - 1)] };
            let columns = self.layout.bar_count(area.width);
            for i in 0..columns {
                let left = self.layout.x_of(i);
                for w in 0..self.layout.bar_width {
                    let x = area.x + left + w;
                    if x >= area.x + area.width {
                        break;
                    }
                    for r in 0..height {
                        let c = grid_color(r);
                        let cell = &mut buf[(x, area.y + height - 1 - r)];
                        if fill {
                            cell.set_symbol(" ").set_fg(c).set_bg(c);
                        } else {
                            cell.set_symbol(self.bar_style.grid_glyph()).set_fg(c);
                        }
                    }
                }
            }
        }

        let drawable = self.layout.bar_count(area.width).min(self.bars.len());
        for i in 0..drawable {
            // Only the blocks style has sub-cell resolution; every other glyph
            // lights whole rows, which is what makes them read as segments.
            let (full, remainder) = if self.bar_style.sub_cell() {
                let eighths = bar_eighths(self.bars[i], height);
                ((eighths / 8) as u16, eighths % 8)
            } else {
                (segment_top(self.bars[i], height) as u16, 0)
            };

            // A cap that has finished falling rests on the floor rather than
            // disappearing. On a 16-pixel panel a cap below one pixel has nowhere
            // to be, but a terminal row is large and a band with no signal in it
            // is worth showing as a mark on the bottom row - the alternative is a
            // column that looks broken rather than quiet.
            let peak = self.peaks.get(i).copied().unwrap_or(0.0);
            let cap = self.peaks_style.cell(peak, height).map(|(row, glyph)| {
                // The cap must read as riding on the bar, never level with
                // it. Keying this off `remainder` only ever worked for the
                // Blocks style, which is the one style that has a partial
                // row; every other style hardcodes remainder to 0, and since
                // `segment_top` rounds while `cap_position` truncates, their
                // caps landed on the bar's own top row roughly half the time.
                // Lift against the first *unlit* row instead, which is the
                // same predicate the drawing loop below uses.
                let first_unlit = if remainder > 0 { full + 1 } else { full };
                if row < first_unlit && first_unlit < height {
                    (first_unlit, glyph)
                } else {
                    (row, glyph)
                }
            });

            let left = self.layout.x_of(i);
            for w in 0..self.layout.bar_width {
                let x = area.x + left + w;
                if x >= area.x + area.width {
                    break;
                }
                for r in 0..height {
                    let lit = r < full || (r == full && remainder > 0);
                    let at = (x, area.y + height - 1 - r);

                    // Symbol and foreground only, so whatever background the grid
                    // pass established for this column stays and a cap or a
                    // partial block never differs from the cells around it.
                    if let Some((_, glyph)) = cap.filter(|(a, _)| *a == r) {
                        buf[at].set_symbol(glyph).set_fg(self.cap_color);
                    } else if lit {
                        let glyph = if r < full {
                            self.bar_style.glyph()
                        } else {
                            self.bar_style.partial(remainder)
                        };
                        buf[at].set_symbol(glyph).set_fg(row_color(r));
                    }
                }
            }
        }
    }
}

/// Foreground colour for every screen row, bottom-up.
///
/// Colour is a function of the row, never of the bar's own value — that is what
/// makes a tall bar and a short one agree on colour where they overlap, which is
/// the look this reproduces. Recomputed only when the height or skin changes.
pub fn row_colors(height: u16, skin: &Skin) -> Vec<Color> {
    stretch(&skin.bars, height)
}

/// Backdrop colour for every screen row, bottom-up.
///
/// Stretched over the height exactly as the lit ramp is, so an unlit row and the
/// lit row that would replace it are always the same stop - the backdrop is a
/// preview of the colour a bar reaches when it gets there.
///
/// This is where a skin's `darken` is applied, because it is the only place that
/// has the terminal's palette. A skin that named `green` gets *this* terminal's
/// green, scaled - still the user's theme, just further down. A terminal that
/// will not say what its green is leaves the colour alone; there is nothing to
/// scale, and inventing one would replace the theme rather than dim it.
pub fn grid_colors(height: u16, skin: &Skin, theme: &Theme) -> Vec<Color> {
    let stops: Vec<Color> = match skin.darken {
        None => skin.grid.clone(),
        Some(floor) => skin
            .grid
            .iter()
            .map(|&c| match theme.rgb(c) {
                // All three channels scale together, which keeps the hue.
                // Scaling per channel drains a saturated colour towards black by
                // way of brown.
                Some((r, g, b)) => Color::Rgb(
                    (r as f32 * floor).round() as u8,
                    (g as f32 * floor).round() as u8,
                    (b as f32 * floor).round() as u8,
                ),
                None => c,
            })
            .collect(),
    };
    stretch(&stops, height)
}

fn stretch(stops: &[Color], height: u16) -> Vec<Color> {
    (0..height)
        .map(|r| stops[ramp_index(r, height, stops.len())])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Imported only here: production code deliberately never draws it, and
    /// `no_cap_is_ever_drawn_at_the_top_of_its_cell` is what keeps that true.
    use crate::ui::scale::CAP_HIGH;
    use ratatui::buffer::Buffer;

    /// The default skin's backdrop colour for a given screen row. It is a ramp,
    /// not one flat colour, so the row matters.
    fn fill_color_at(y: u16, height: u16) -> Color {
        let colors = grid_colors(height, &Skin::default(), &Theme::default());
        colors[(height - 1 - y) as usize]
    }

    /// The default skin's cap colour.
    fn cap_color() -> Color {
        Skin::default().peak
    }

    /// For tests about the bars themselves. Caps rest on the bottom row now, so
    /// anything asserting on that row has to say it does not want them.
    fn render_bars(bars: &[f32], w: u16, h: u16, grid: bool) -> Buffer {
        render_full(
            bars,
            &vec![0.0; bars.len()],
            w,
            h,
            grid,
            BarStyle::Blocks,
            Peaks::Off,
        )
    }

    fn render(bars: &[f32], peaks: &[f32], w: u16, h: u16, grid: bool) -> Buffer {
        render_styled(bars, peaks, w, h, grid, BarStyle::Blocks)
    }

    fn render_styled(
        bars: &[f32],
        peaks: &[f32],
        w: u16,
        h: u16,
        grid: bool,
        style: BarStyle,
    ) -> Buffer {
        render_full(bars, peaks, w, h, grid, style, Peaks::Fine)
    }

    fn render_full(
        bars: &[f32],
        peaks: &[f32],
        w: u16,
        h: u16,
        grid: bool,
        style: BarStyle,
        peaks_style: Peaks,
    ) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let skin = Skin::default();
        let colors = row_colors(h, &skin);
        let backdrop = grid.then(|| grid_colors(h, &skin, &Theme::default()));
        Analyzer {
            bars,
            peaks,
            row_colors: &colors,
            cap_color: skin.peak,
            grid: backdrop.as_deref(),
            bar_style: style,
            layout: BarLayout {
                bar_width: 1,
                spacing: 0,
            },
            peaks_style,
        }
        .render(area, &mut buf);
        buf
    }

    #[test]
    fn a_full_bar_fills_its_column() {
        let buf = render(&[1.0], &[1.0], 1, 8, false);
        for y in 1..8 {
            assert_eq!(buf[(0, y)].symbol(), "█", "row {y}");
        }
    }

    #[test]
    fn partial_bars_use_eighth_blocks() {
        // 0.55 of 8 = 4.4 rows: four full cells and a 3/8 top. The value has to
        // be one that does not divide evenly, or no partial cell is produced and
        // nothing pins BLOCKS[remainder].
        let buf = render_bars(&[0.55], 1, 8, false);
        assert_eq!(buf[(0, 7)].symbol(), "█", "bottom row full");
        assert_eq!(buf[(0, 4)].symbol(), "█", "fourth row full");
        assert_eq!(buf[(0, 3)].symbol(), "▃", "top of the bar is a 3/8 block");
        assert_eq!(buf[(0, 2)].symbol(), " ", "nothing above it");
    }

    #[test]
    fn colour_depends_on_the_row_not_the_bar_height() {
        // The defining property of this display. Two bars of very different heights must
        // agree on colour wherever they both have a cell.
        let buf = render_bars(&[1.0, 0.3], 2, 16, false);
        for y in 12..16 {
            assert_eq!(buf[(0, y)].fg, buf[(1, y)].fg, "colour differed at row {y}");
        }
    }

    #[test]
    fn the_ramp_runs_green_at_the_bottom_to_red_at_the_top() {
        let buf = render_bars(&[1.0], 1, 16, false);
        assert_eq!(buf[(0, 15)].fg, Skin::default().bars[0], "bottom is green");
        assert_eq!(
            buf[(0, 0)].fg,
            *Skin::default().bars.last().unwrap(),
            "top is red"
        );
    }

    #[test]
    fn a_cap_never_sits_level_with_the_bar_it_belongs_to() {
        // Where the cap would share the bar's partial top cell it is lifted a
        // row, so it always reads as riding on the bar rather than inside it.
        let h = 16;
        let buf = render(&[0.53], &[0.53], 1, h, false);
        let cap_row = (0..h)
            .find(|&y| buf[(0, y)].fg == cap_color())
            .expect("a cap should be drawn");
        let top_lit = (0..h)
            .find(|&y| buf[(0, y)].symbol() != " " && buf[(0, y)].fg != cap_color())
            .expect("a bar should be drawn");
        assert!(
            cap_row < top_lit,
            "cap at row {cap_row}, bar top at {top_lit}"
        );
    }

    #[test]
    fn every_bar_style_keeps_its_cap_clear_of_the_bar() {
        // The lift is keyed off the first unlit row rather than off `remainder`,
        // which only the Blocks style sets - every style has to clear its cap,
        // not just the one that reports a partial cell.
        for style in [
            BarStyle::Shade,
            BarStyle::Blocks,
            BarStyle::Solid,
            BarStyle::Thick,
            BarStyle::Half,
            BarStyle::Line,
        ] {
            for value in [0.44f32, 0.5, 0.62, 0.77] {
                let buf = render_styled(&[value], &[value], 1, 8, false, style);
                let cap = (0..8)
                    .find(|&y| buf[(0, y)].fg == cap_color())
                    .unwrap_or_else(|| panic!("{} {value}: no cap drawn", style.label()));
                // Rows count down from the top, so the cap must have a strictly
                // smaller index than anything the bar lit.
                let bar_top = (0..8)
                    .find(|&y| buf[(0, y)].symbol() != " " && buf[(0, y)].fg != cap_color())
                    .unwrap_or_else(|| panic!("{} {value}: no bar drawn", style.label()));
                assert!(
                    cap < bar_top,
                    "{} at {value}: cap at row {cap}, bar top at {bar_top}",
                    style.label()
                );
            }
        }
    }

    #[test]
    fn a_short_row_colors_slice_does_not_panic() {
        // ratatui re-queries the terminal size inside draw(), so a resize between
        // measuring and rendering leaves row_colors shorter than the area. A
        // panic here would unwind past the raw-mode teardown and wreck the
        // terminal, so the index is clamped instead.
        let area = Rect::new(0, 0, 1, 40);
        let mut buf = Buffer::empty(area);
        let colors = row_colors(24, &Skin::default()); // deliberately short
        Analyzer {
            bars: &[1.0],
            peaks: &[1.0],
            row_colors: &colors,
            cap_color: cap_color(),
            grid: None,
            bar_style: BarStyle::Blocks,
            layout: BarLayout {
                bar_width: 1,
                spacing: 0,
            },
            peaks_style: Peaks::Fine,
        }
        .render(area, &mut buf);
    }

    #[test]
    fn a_full_height_bar_keeps_its_cap_on_the_top_row() {
        // Nowhere to lift it to; the cap stays and the bar colour shows behind.
        let buf = render(&[1.0], &[1.0], 1, 8, false);
        assert_eq!(buf[(0, 0)].fg, cap_color());
    }

    #[test]
    fn a_cap_above_the_bar_does_not_invent_a_bar_colour() {
        // Only where the cap overlaps a lit row; floating above it must not.
        let buf = render(&[0.1], &[1.0], 1, 8, false);
        assert_eq!(buf[(0, 0)].bg, Color::Reset);
    }

    #[test]
    fn a_cap_draws_over_the_bar_beneath_it() {
        let buf = render(&[1.0], &[1.0], 1, 8, false);
        assert!(
            [CAP_LOW, CAP_MID, CAP_HIGH].contains(&buf[(0, 0)].symbol()),
            "cap wins over a full bar"
        );
        assert_eq!(buf[(0, 0)].fg, cap_color(), "cap is grey");
    }

    #[test]
    fn a_cap_rests_on_the_floor_rather_than_vanishing() {
        // A band with nothing in it still has a peak, and it belongs on the
        // bottom row. Hiding it leaves a gap in the row of caps that reads as
        // something broken rather than as a quiet band.
        let buf = render(&[0.0], &[0.0], 1, 16, false);
        assert_eq!(
            buf[(0, 15)].symbol(),
            CAP_LOW,
            "the bottom row should carry the cap"
        );
        assert_eq!(buf[(0, 15)].fg, cap_color());
    }

    #[test]
    fn partial_blocks_and_caps_sit_on_the_grid_colour() {
        // Both cover only part of their cell, so without the grid colour behind
        // them the terminal shows through - and under the cap that hole tracks
        // the peak up and down, which is more distracting than the cap itself.
        // 0.3 of 8 rows leaves a partial block; the cap sits above it.
        let buf = render_styled(&[0.3], &[0.9], 1, 8, true, BarStyle::Blocks);
        let partial = (0..8).find(|&y| {
            let sym = buf[(0, y)].symbol();
            sym != "█" && sym != " " && ![CAP_LOW, CAP_MID, CAP_HIGH].contains(&sym)
        });
        assert!(partial.is_some(), "expected a partial block somewhere");
        let partial = partial.unwrap();
        assert_eq!(
            buf[(0, partial)].bg,
            fill_color_at(partial, 8),
            "partial block bg"
        );
        let cap = (0..8)
            .find(|&y| [CAP_LOW, CAP_MID, CAP_HIGH].contains(&buf[(0, y)].symbol()))
            .unwrap();
        assert_eq!(buf[(0, cap)].bg, fill_color_at(cap, 8), "cap bg");
    }

    #[test]
    fn every_cell_takes_the_backdrop_colour_of_its_own_row() {
        // The backdrop is a ramp, so each row has its own colour - but every cell
        // in a row must carry it whether it is lit, unlit or capped. A cell that
        // differs is a patch, and under the cap it slides down the screen.
        for style in [BarStyle::Blocks, BarStyle::Shade] {
            let buf = render_styled(&[0.55], &[0.9], 1, 8, true, style);
            for y in 0..8u16 {
                assert_eq!(
                    buf[(0, y)].bg,
                    fill_color_at(y, 8),
                    "{} row {y} does not match its backdrop",
                    style.label()
                );
            }
        }
    }

    #[test]
    fn with_the_grid_off_the_terminal_background_is_left_alone() {
        let buf = render_styled(&[0.3], &[0.9], 1, 8, false, BarStyle::Blocks);
        for y in 0..8 {
            assert_eq!(buf[(0, y)].bg, Color::Reset, "row {y} must stay untouched");
        }
    }

    #[test]
    fn a_lit_bar_paints_over_its_unlit_segments() {
        let buf = render_styled(&[1.0], &[0.0], 1, 8, true, BarStyle::Blocks);
        for y in 1..8u16 {
            assert_ne!(
                buf[(0, y)].fg,
                fill_color_at(y, 8),
                "row {y} should be lit, not grid"
            );
        }
    }

    #[test]
    fn bars_paint_over_the_grid() {
        // Colour is what distinguishes them, not the glyph: the unlit grid draws
        // the same symbol as a lit Blocks bar, so a symbol assertion passes even
        // when no bar is drawn at all.
        let buf = render(&[1.0], &[0.0], 1, 8, true);
        assert_ne!(
            buf[(0, 6)].fg,
            fill_color_at(6, 8),
            "row is still the unlit colour - the bar did not paint over it"
        );
    }

    #[test]
    fn degenerate_areas_do_not_panic() {
        for (w, h) in [(0u16, 0u16), (1, 1), (1, 80), (200, 1), (3, 2)] {
            let _ = render(&[0.5, 0.9], &[0.7, 1.0], w, h, true);
        }
    }

    #[test]
    fn each_peak_level_subdivides_the_row_further() {
        // 1, 2 and 3 positions per row. The row itself must agree across all of
        // them - only the glyph changes.
        let at = |p: Peaks, v: f32| p.cell(v, 10).unwrap();
        assert_eq!(at(Peaks::Coarse, 0.51).1, CAP_MID);
        assert_eq!(
            at(Peaks::Coarse, 0.59).1,
            CAP_MID,
            "coarse never subdivides"
        );

        assert_eq!(
            at(Peaks::Fine, 0.51).1,
            CAP_LOW,
            "low half sits on the edge"
        );
        assert_eq!(at(Peaks::Fine, 0.59).1, CAP_MID, "high half is mid-cell");

        for style in [Peaks::Coarse, Peaks::Fine] {
            assert_eq!(
                at(style, 0.55).0,
                5,
                "{} disagreed on the row",
                style.label()
            );
        }
        assert_eq!(Peaks::Off.cell(0.55, 10), None);
    }

    #[test]
    fn no_cap_is_ever_drawn_at_the_top_of_its_cell() {
        // The upper one-eighth block detaches the cap from the bar, and on the
        // display's top row presses it against the screen edge.
        for style in [Peaks::Coarse, Peaks::Fine] {
            for step in 0..60 {
                let (_, glyph) = style.cell(step as f32 / 60.0, 10).unwrap();
                assert_ne!(glyph, CAP_HIGH, "{} at step {step}", style.label());
            }
        }
    }

    #[test]
    fn peaks_cycle_through_every_level() {
        let mut p = Peaks::default();
        assert_eq!(p, Peaks::Fine, "finest is the default");
        let mut seen = vec![p.label()];
        for _ in 0..2 {
            p = p.next();
            seen.push(p.label());
        }
        assert_eq!(seen, vec!["fine", "coarse", "off"]);
        assert_eq!(p.next(), Peaks::Fine, "cycles back round");
    }

    #[test]
    fn the_default_bar_style_climbs_by_eighths() {
        // Eight steps within a cell is most of the vertical resolution a terminal
        // has; the whole-row styles are a keypress away.
        assert_eq!(BarStyle::default(), BarStyle::Blocks);
        assert_eq!(BarStyle::Blocks.glyph(), BLOCKS[8]);
    }

    #[test]
    fn the_shade_style_resolves_below_a_row_by_density() {
        // No shaded eighth-blocks exist, so the top cell fades in through
        // `░▒▓` instead of growing upward. Three steps rather than eight.
        let seen: Vec<&str> = [0.30f32, 0.36, 0.42]
            .iter()
            .map(|&v| {
                let buf = render_full(&[v], &[0.0], 1, 8, false, BarStyle::Shade, Peaks::Off);
                // The topmost lit cell, whatever it is.
                (0..8)
                    .find(|&y| buf[(0, y)].symbol() != " ")
                    .map(|y| match buf[(0, y)].symbol() {
                        "░" => "light",
                        "▒" => "medium",
                        "▓" => "dark",
                        other => panic!("unexpected glyph {other:?}"),
                    })
                    .expect("something should be drawn")
            })
            .collect();
        assert!(
            seen.windows(2).any(|w| w[0] != w[1]),
            "the top cell never changed density: {seen:?}"
        );
    }

    #[test]
    fn the_whole_row_styles_never_show_a_partial_cell() {
        // Their whole point: a lit row is lit, which is what makes them read as
        // discrete segments. Only the two full-cell styles resolve below a row.
        for style in [
            BarStyle::Solid,
            BarStyle::Thick,
            BarStyle::Half,
            BarStyle::Line,
        ] {
            assert!(!style.sub_cell(), "{} claims sub-cell", style.label());
            let buf = render_full(&[0.55], &[0.0], 1, 8, false, style, Peaks::Off);
            let drawn: Vec<&str> = (0..8).map(|y| buf[(0, y)].symbol()).collect();
            assert!(
                drawn.iter().all(|&g| g == " " || g == style.glyph()),
                "{} drew a partial cell: {drawn:?}",
                style.label()
            );
        }
    }

    #[test]
    fn every_bar_style_labels_itself_and_cycles() {
        let mut style = BarStyle::default();
        let mut seen = vec![style.label()];
        for _ in 0..5 {
            style = style.next();
            seen.push(style.label());
        }
        assert_eq!(
            seen,
            vec!["blocks", "solid", "thick", "half", "line", "shade"]
        );
        assert_eq!(style.next(), BarStyle::Blocks, "cycles back round");
    }

    #[test]
    fn darkening_scales_the_backdrop_and_keeps_its_hue() {
        // All three channels together, never per channel: a saturated colour has
        // to stay itself on the way down, not drift towards brown.
        let skin = Skin {
            grid: vec![Color::Rgb(240, 48, 16); 16],
            darken: Some(0.25),
            ..Skin::default()
        };
        let colors = grid_colors(16, &skin, &Theme::default());
        assert_eq!(
            colors[0],
            Color::Rgb(60, 12, 4),
            "each channel by the floor"
        );
    }

    #[test]
    fn a_named_backdrop_is_darkened_through_the_terminals_own_palette() {
        // The skin said `green`, so the answer has to be *this* terminal's green
        // taken down - not a green rav picked.
        let skin = Skin {
            grid: vec![Color::Green; 16],
            darken: Some(0.5),
            ..Skin::default()
        };

        // A terminal that will not say leaves the colour alone; inventing one
        // would replace the theme rather than dim it.
        let silent = grid_colors(16, &skin, &Theme::default());
        assert_eq!(silent[0], Color::Green);
    }

    #[test]
    fn the_grid_is_a_background_fill_for_the_full_cell_styles() {
        // Not a block glyph: those leave a hairline gap between cells in most
        // terminal fonts, so any cell that is filled instead - the cap's, or a
        // partial top - stands out and slides down the column with the cap.
        for style in [BarStyle::Blocks, BarStyle::Shade] {
            let area = Rect::new(0, 0, 1, 4);
            let mut buf = Buffer::empty(area);
            let skin = Skin::default();
            let colors = row_colors(4, &skin);
            Analyzer {
                bars: &[0.0],
                peaks: &[0.0],
                row_colors: &colors,
                cap_color: cap_color(),
                grid: Some(&[Color::Rgb(24, 33, 41); 4]),
                bar_style: style,
                layout: BarLayout {
                    bar_width: 1,
                    spacing: 0,
                },
                peaks_style: Peaks::Off,
            }
            .render(area, &mut buf);
            for y in 0..4u16 {
                assert_eq!(
                    buf[(0, y)].symbol(),
                    " ",
                    "{} drew a glyph instead of filling row {y}",
                    style.label()
                );
                assert_eq!(
                    buf[(0, y)].bg,
                    Color::Rgb(24, 33, 41),
                    "{} left row {y} unfilled",
                    style.label()
                );
            }
        }
    }

    #[test]
    fn a_partial_style_keeps_its_own_shape_in_the_grid_and_takes_no_background() {
        // A solid backdrop behind a ladder of thin rules would be a slab, and the
        // filled cell under the cap would ride up and down through it.
        for style in [
            BarStyle::Solid,
            BarStyle::Thick,
            BarStyle::Half,
            BarStyle::Line,
        ] {
            let buf = render_styled(&[0.3], &[0.9], 1, 8, true, style);
            // Row 3 is neither lit (0.3 of 8 reaches two rows) nor the cap's.
            assert_eq!(
                buf[(0, 3)].symbol(),
                style.glyph(),
                "{} grid should keep its own shape",
                style.label()
            );
            let cap = (0..8)
                .find(|&y| [CAP_LOW, CAP_MID].contains(&buf[(0, y)].symbol()))
                .unwrap_or_else(|| panic!("{}: no cap drawn", style.label()));
            assert_eq!(
                buf[(0, cap)].bg,
                Color::Reset,
                "{} cap should not be filled",
                style.label()
            );
        }
    }

    #[test]
    fn layout_counts_bars_without_a_trailing_gap() {
        let l = BarLayout {
            bar_width: 3,
            spacing: 1,
        };
        // 3 + 1 + 3 = 7 columns holds two bars.
        assert_eq!(l.bar_count(7), 2);
        assert_eq!(l.bar_count(3), 1);
        assert_eq!(l.x_of(2), 8);
    }

    #[test]
    fn more_bars_than_fit_are_truncated_not_wrapped() {
        // Assert on the cells, not on buf.area(): the area is fixed by the
        // caller, so a test against it passes even when render() draws nothing.
        let buf = render(&[1.0; 50], &[0.0; 50], 4, 4, false);
        for x in 0..4u16 {
            assert_eq!(buf[(x, 0)].symbol(), "█", "column {x} should be filled");
        }
    }
}
