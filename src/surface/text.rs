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

    /// One row per pixel of height, most significant bit leftmost. `None` for a
    /// character the font does not carry, which is drawn as nothing rather than
    /// as a substitute - a box glyph in the middle of a help panel reads as
    /// data corruption.
    fn rows(&self, ch: char) -> Option<&[u8]>;
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
                    if bits & (0x80 >> col) == 0 {
                        col += 1;
                        continue;
                    }
                    let start = col;
                    while col < across.min(8) && bits & (0x80 >> col) != 0 {
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
