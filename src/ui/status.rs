//! The short note shown after a settings key.

use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

/// A brief message in the top-left corner.
///
/// Its own component rather than something written into the analyser's buffer,
/// for two reasons. A pixel surface sends the bars as an image and writes the
/// overlay as text, and a cell written over a kitty image blanks it - so the two
/// have to be renderable apart or the frame is erased by its own status line.
/// And a note that outlives one keypress should not require the analyser to be
/// redrawn to move.
///
/// Overlaid rather than given a row of its own: a permanent status bar would
/// cost a row of bars for a message that is empty most of the time.
pub struct Status<'a> {
    pub text: &'a str,
    pub foreground: Color,
    /// The active theme's floor colour, so the note sits on the backdrop rather
    /// than punching a hole in it.
    pub background: Color,
}

impl Widget for Status<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        // Padded, so the label reads as a plate rather than as text that
        // happens to start against the left edge.
        let label = format!(" {} ", self.text);
        for (offset, character) in label.chars().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let x = area.x.saturating_add(offset);
            if x >= area.x.saturating_add(area.width) {
                break;
            }
            buf[(x, area.y)]
                .set_symbol(&character.to_string())
                .set_fg(self.foreground)
                .set_bg(self.background);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn rendered(text: &str, width: u16) -> Buffer {
        let area = Rect::new(0, 0, width, 3);
        let mut buf = Buffer::empty(area);
        Status {
            text,
            foreground: Color::Rgb(222, 222, 222),
            background: Color::Rgb(0, 32, 0),
        }
        .render(area, &mut buf);
        buf
    }

    fn row(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width).map(|x| buf[(x, y)].symbol()).collect()
    }

    #[test]
    fn the_note_is_padded_so_it_reads_as_a_plate() {
        let buf = rendered("gain +3 dB", 20);
        assert_eq!(row(&buf, 0, 12), " gain +3 dB ");
    }

    #[test]
    fn it_occupies_one_row_and_leaves_the_rest_alone() {
        // The reason it is an overlay: a permanent bar would cost a row of bars
        // for a message that is empty most of the time.
        let buf = rendered("peaks off", 20);
        assert_eq!(row(&buf, 1, 20).trim(), "", "the second row is untouched");
        assert_eq!(row(&buf, 2, 20).trim(), "");
    }

    #[test]
    fn a_note_wider_than_the_screen_is_cut_rather_than_wrapped() {
        // Wrapping would push the note over a second row of bars, and a
        // truncated note still says which setting moved.
        let buf = rendered("a very long message indeed", 8);
        assert_eq!(row(&buf, 0, 8), " a very ");
        assert_eq!(row(&buf, 1, 8).trim(), "", "nothing spilled onto row two");
    }

    #[test]
    fn it_paints_the_background_it_was_given() {
        // The note follows the active theme rather than a hardcoded default, so
        // it sits on the backdrop instead of punching a hole in it.
        let buf = rendered("x", 8);
        assert_eq!(buf[(0, 0)].bg, Color::Rgb(0, 32, 0));
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(222, 222, 222));
    }

    #[test]
    fn an_empty_area_draws_nothing_rather_than_panicking() {
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        Status {
            text: "ignored",
            foreground: Color::White,
            background: Color::Black,
        }
        .render(area, &mut buf);
    }
}
