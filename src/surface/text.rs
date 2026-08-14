//! Drawing text into a frame, for the surfaces that have no terminal to do it.
//!
//! A window has no ratatui and no font stack - `usvg` is built without one on
//! purpose - so the help panel and anything else with words in it are drawn a
//! glyph at a time out of a bitmap.
//!
//! The font is not here. This knows how to place glyphs and nothing about what
//! they look like: a [`Bitmap`] is whatever supplies rows of bits, and which
//! crate does that is a separate decision from any of the geometry below.

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use rav_appearance::ink::Colour;

/// A monospaced bitmap font: fixed cell, one bit per pixel, rows top-first.
///
/// Deliberately a trait over "give me the rows for this character" rather than
/// a concrete array. Every candidate crate stores its glyphs differently - a
/// flat table, PSF-2 blobs, per-size modules - and all of them can answer this.
pub trait Bitmap {
    /// Cell size in pixels, the same for every glyph.
    fn cell(&self) -> (u32, u32);

    /// One row per pixel of height. `None` for a character the font does not
    /// carry, which is drawn as nothing rather than as a substitute - a box
    /// glyph in the middle of a help panel reads as data corruption.
    fn rows(&self, ch: char) -> Option<&[u8]>;

    /// Which end of a row byte holds the leftmost pixel.
    ///
    /// Fonts disagree and neither is wrong: `font8x8` puts it in bit 0.
    /// Getting it backwards mirrors every glyph, and a symmetric one hides
    /// that completely - `A` reads correctly either way and `L` does not,
    /// which is why this is asked rather than assumed.
    fn leftmost(&self) -> Leftmost {
        Leftmost::HighBit
    }
}

/// Where a row byte keeps its leftmost pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leftmost {
    /// Bit 7 first, reading the byte as it is written.
    HighBit,
    /// Bit 0 first, which is what `font8x8` does.
    LowBit,
}

impl Leftmost {
    /// The mask for a column, counted from the left.
    fn mask(self, column: u32) -> u8 {
        match self {
            Leftmost::HighBit => 0x80 >> column,
            Leftmost::LowBit => 1 << column,
        }
    }
}

/// Draw one line of text with its top-left corner at `x`, `y`.
///
/// Returns where the next character would start, so a caller can put two
/// colours on one line without measuring anything twice.
///
/// Whole pixels throughout. A bitmap glyph placed on a fractional boundary is
/// resampled into a blur, which is the one thing a bitmap font is chosen to
/// avoid - and #63 in this repository is what rounding did the last time.
pub fn draw(
    onto: &mut Pixmap,
    font: &dyn Bitmap,
    text: &str,
    (x, y): (u32, u32),
    colour: Colour,
) -> u32 {
    let (across, down) = font.cell();
    let mut paint = Paint::default();
    paint.set_color_rgba8(colour.red, colour.green, colour.blue, colour.alpha);
    paint.anti_alias = false;

    let leftmost = font.leftmost();
    let mut at = x;
    for ch in text.chars() {
        if let Some(rows) = font.rows(ch) {
            for (row, bits) in rows.iter().take(down as usize).enumerate() {
                // A run at a time rather than a pixel at a time: a filled span
                // is one rectangle, and a help panel is mostly horizontal
                // strokes. `bits` is 8 wide, so this is at most four rects a
                // row instead of eight.
                let mut col = 0u32;
                while col < across.min(8) {
                    if bits & leftmost.mask(col) == 0 {
                        col += 1;
                        continue;
                    }
                    let start = col;
                    while col < across.min(8) && bits & leftmost.mask(col) != 0 {
                        col += 1;
                    }
                    if let Some(run) = Rect::from_xywh(
                        (at + start) as f32,
                        (y + row as u32) as f32,
                        (col - start) as f32,
                        1.0,
                    ) {
                        onto.fill_rect(run, &paint, Transform::identity(), None);
                    }
                }
            }
        }
        at += across;
    }
    at
}

/// How wide that text is, in pixels.
pub fn width(font: &dyn Bitmap, text: &str) -> u32 {
    font.cell().0 * text.chars().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font of two glyphs, so the geometry can be checked without deciding
    /// which real one rav ships. `#` is solid, `|` is a single left column.
    struct Probe;

    impl Bitmap for Probe {
        fn cell(&self) -> (u32, u32) {
            (8, 4)
        }
        fn rows(&self, ch: char) -> Option<&[u8]> {
            match ch {
                '#' => Some(&[0xff, 0xff, 0xff, 0xff]),
                '|' => Some(&[0x80, 0x80, 0x80, 0x80]),
                _ => None,
            }
        }
    }

    fn lit(map: &Pixmap, x: u32, y: u32) -> bool {
        map.pixel(x, y).is_some_and(|p| p.alpha() > 0)
    }

    fn white() -> Colour {
        Colour {
            red: 0xff,
            green: 0xff,
            blue: 0xff,
            alpha: 0xff,
        }
    }

    #[test]
    fn a_glyph_lands_in_its_own_cell_and_nowhere_else() {
        let mut map = Pixmap::new(24, 8).unwrap();
        draw(&mut map, &Probe, "#", (0, 0), white());

        assert!(lit(&map, 0, 0) && lit(&map, 7, 3), "the cell is not filled");
        assert!(!lit(&map, 8, 0), "it bled into the next cell across");
        assert!(!lit(&map, 0, 4), "it bled into the row below");
    }

    #[test]
    fn the_second_character_starts_one_cell_along() {
        let mut map = Pixmap::new(24, 8).unwrap();
        let end = draw(&mut map, &Probe, "||", (0, 0), white());

        assert!(lit(&map, 0, 0), "the first stroke");
        assert!(lit(&map, 8, 0), "the second, one cell along");
        assert!(
            !lit(&map, 1, 0),
            "a single-column glyph filled more than one"
        );
        assert_eq!(end, 16, "the caller is told where to carry on");
    }

    #[test]
    fn an_unknown_character_draws_nothing_and_still_takes_its_place() {
        // Nothing rather than a substitute: a box glyph in the middle of a help
        // panel reads as corruption, where a gap reads as a gap.
        let mut map = Pixmap::new(24, 8).unwrap();
        let end = draw(&mut map, &Probe, "?#", (0, 0), white());

        assert!(!lit(&map, 0, 0), "the unknown one drew something");
        assert!(lit(&map, 8, 0), "the known one moved out of its way");
        assert_eq!(end, 16);
    }

    #[test]
    fn an_offset_is_whole_pixels_and_exact() {
        let mut map = Pixmap::new(24, 8).unwrap();
        draw(&mut map, &Probe, "#", (3, 2), white());

        assert!(lit(&map, 3, 2) && lit(&map, 10, 5), "the corners");
        assert!(!lit(&map, 2, 2), "one column left of where it was put");
        assert!(!lit(&map, 3, 1), "one row above");
    }

    #[test]
    fn width_is_what_draw_uses() {
        // The two agree, or a panel measured with one and drawn with the other
        // is a border that does not fit its contents.
        let mut map = Pixmap::new(64, 8).unwrap();
        let end = draw(&mut map, &Probe, "###", (0, 0), white());
        assert_eq!(end, width(&Probe, "###"));
    }

    #[test]
    fn the_bit_order_is_asked_for_and_not_assumed() {
        // The trap this exists for: `A` reads correctly whichever end the
        // leftmost pixel is at, so a mirrored font ships looking fine until
        // somebody types an `L`. One glyph, one edge lit, read both ways.
        struct OneEdge(Leftmost);
        impl Bitmap for OneEdge {
            fn cell(&self) -> (u32, u32) {
                (8, 1)
            }
            fn rows(&self, _: char) -> Option<&[u8]> {
                Some(&[0x01])
            }
            fn leftmost(&self) -> Leftmost {
                self.0
            }
        }

        let mut high = Pixmap::new(8, 1).unwrap();
        draw(&mut high, &OneEdge(Leftmost::HighBit), "x", (0, 0), white());
        assert!(
            lit(&high, 7, 0) && !lit(&high, 0, 0),
            "bit 0 is the right edge"
        );

        let mut low = Pixmap::new(8, 1).unwrap();
        draw(&mut low, &OneEdge(Leftmost::LowBit), "x", (0, 0), white());
        assert!(
            lit(&low, 0, 0) && !lit(&low, 7, 0),
            "bit 0 is the left edge"
        );
    }

    #[test]
    fn a_glyph_taller_than_the_cell_is_cut_to_it() {
        // A font whose rows outnumber its declared height would otherwise draw
        // over the line below, which is invisible until two lines are adjacent.
        struct TooTall;
        impl Bitmap for TooTall {
            fn cell(&self) -> (u32, u32) {
                (8, 2)
            }
            fn rows(&self, _: char) -> Option<&[u8]> {
                Some(&[0xff, 0xff, 0xff, 0xff])
            }
        }
        let mut map = Pixmap::new(8, 8).unwrap();
        draw(&mut map, &TooTall, "x", (0, 0), white());
        assert!(lit(&map, 0, 1), "inside the cell");
        assert!(!lit(&map, 0, 2), "past the declared height");
    }
}

/// Draw the help panel into a frame.
///
/// The layout is the terminal's - [`Help::panel`] decides where it sits and how
/// big it is, in cells, and this multiplies by the font's cell to reach pixels.
/// Two implementations of "how wide is the key column" would be a window whose
/// panel disagrees with a terminal's about which rows fit.
///
/// `screen` is in pixels; the cell comes from the font.
pub fn help(
    onto: &mut Pixmap,
    font: &dyn Bitmap,
    rows: &[crate::ui::help::HelpRow<'_>],
    title: &str,
    (screen_across, screen_down): (u32, u32),
    (ink, background): (Colour, Colour),
) {
    let (across, down) = font.cell();
    if across == 0 || down == 0 {
        return;
    }
    let panel = crate::ui::help::Help { rows, title };
    // The terminal's own sizing, asked in cells: how many of them fit across
    // and down this window at the font's size.
    let area = ratatui::layout::Rect {
        x: 0,
        y: 0,
        width: (screen_across / across) as u16,
        height: (screen_down / down) as u16,
    };
    let at = panel.panel(area);
    if at.width == 0 || at.height == 0 {
        return;
    }

    let mut paint = Paint::default();
    paint.set_color_rgba8(
        background.red,
        background.green,
        background.blue,
        background.alpha,
    );
    paint.anti_alias = false;
    if let Some(behind) = Rect::from_xywh(
        (u32::from(at.x) * across) as f32,
        (u32::from(at.y) * down) as f32,
        (u32::from(at.width) * across) as f32,
        (u32::from(at.height) * down) as f32,
    ) {
        onto.fill_rect(behind, &paint, Transform::identity(), None);
    }

    let left = u32::from(at.x) * across;
    let top = u32::from(at.y) * down;
    draw(onto, font, title, (left + across * 2, top + down), ink);

    // Two cells in from the border and one line below the title, which is what
    // the terminal panel does - the two are meant to be the same panel.
    let key_width = panel.key_width() as u32;
    for (line, row) in rows.iter().enumerate() {
        let y = top + down * (line as u32 + 3);
        if y + down > top + u32::from(at.height) * down {
            break;
        }
        draw(onto, font, row.key, (left + across * 2, y), ink);
        let description = left + across * (2 + key_width + 2);
        let after = draw(onto, font, row.description, (description, y), ink);
        if let Some(value) = &row.value {
            draw(onto, font, value, (after + across, y), ink);
        }
    }
}

#[cfg(test)]
mod panel_tests {
    use super::*;
    use crate::ui::help::HelpRow;

    /// The same two-glyph font the tests above use, declared again rather than
    /// reached for across module walls.
    struct Probe;
    impl Bitmap for Probe {
        fn cell(&self) -> (u32, u32) {
            (8, 4)
        }
        fn rows(&self, ch: char) -> Option<&[u8]> {
            match ch {
                '#' => Some(&[0xff, 0xff, 0xff, 0xff]),
                '|' => Some(&[0x80, 0x80, 0x80, 0x80]),
                _ => None,
            }
        }
    }

    fn white() -> Colour {
        Colour {
            red: 0xff,
            green: 0xff,
            blue: 0xff,
            alpha: 0xff,
        }
    }

    fn grey() -> Colour {
        Colour {
            red: 0x20,
            green: 0x20,
            blue: 0x20,
            alpha: 0xff,
        }
    }

    fn painted(map: &Pixmap, x: u32, y: u32) -> bool {
        map.pixel(x, y).is_some_and(|p| p.alpha() > 0)
    }

    fn rows() -> Vec<HelpRow<'static>> {
        vec![
            HelpRow {
                key: "#",
                description: "##",
                value: Some("#".to_string()),
            },
            HelpRow {
                key: "|",
                description: "##",
                value: None,
            },
        ]
    }

    #[test]
    fn the_panel_stays_inside_its_own_box() {
        let mut map = Pixmap::new(240, 120).unwrap();
        help(
            &mut map,
            &Probe,
            &rows(),
            "##",
            (240, 120),
            (white(), grey()),
        );

        // Something was drawn, and nothing outside the panel the layout chose.
        let panel = crate::ui::help::Help {
            rows: &rows(),
            title: "##",
        }
        .panel(ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: 240 / 8,
            height: 120 / 4,
        });
        let (x0, y0) = (u32::from(panel.x) * 8, u32::from(panel.y) * 4);
        let (x1, y1) = (
            x0 + u32::from(panel.width) * 8,
            y0 + u32::from(panel.height) * 4,
        );

        assert!(painted(&map, x0, y0), "the panel background is not there");
        if x0 > 0 {
            assert!(!painted(&map, x0 - 1, y0), "it painted left of itself");
        }
        if y0 > 0 {
            assert!(!painted(&map, x0, y0 - 1), "it painted above itself");
        }
        assert!(!painted(&map, x1, y0), "it painted right of itself");
        assert!(!painted(&map, x0, y1), "it painted below itself");
    }

    #[test]
    fn a_window_too_small_draws_nothing_rather_than_a_ruin() {
        // Fewer pixels than one cell. The terminal panel shrinks to fit; here
        // there is nothing to shrink into, and half a border is worse than no
        // panel.
        let mut map = Pixmap::new(4, 2).unwrap();
        help(&mut map, &Probe, &rows(), "##", (4, 2), (white(), grey()));
        assert!(!painted(&map, 0, 0), "it drew into a window with no room");
    }

    #[test]
    fn a_font_with_no_size_is_declined() {
        struct Nothing;
        impl Bitmap for Nothing {
            fn cell(&self) -> (u32, u32) {
                (0, 0)
            }
            fn rows(&self, _: char) -> Option<&[u8]> {
                None
            }
        }
        // A zero cell would divide by zero working out how many fit.
        let mut map = Pixmap::new(64, 64).unwrap();
        help(
            &mut map,
            &Nothing,
            &rows(),
            "x",
            (64, 64),
            (white(), grey()),
        );
        assert!(!painted(&map, 0, 0));
    }
}
