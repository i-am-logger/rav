//! Writing the status line and the help panel over a picture made of pixels.
//!
//! A written cell blanks the image beneath it - any cell, at any z, even a
//! space at the default background. So a pixel surface cannot hand its frame to
//! ratatui and let it paint: `Terminal::draw` writes every cell in the buffer,
//! and the picture would be erased the instant it was drawn.
//!
//! What it can do is let the same widgets render into a buffer of their own and
//! then emit **only the cells they actually touched**. The overlay looks
//! identical either way, because it is the same widget; the rest of the grid is
//! never written, so the frame under it survives.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use std::fmt::Write as _;

/// Render `widget` over `area` and return the escape sequence for the cells it
/// used - nothing at all when it drew nothing.
pub fn of(widget: impl Widget, area: Rect) -> String {
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);
    written(&buffer)
}

/// The escapes for every cell a widget left something in.
///
/// "Something" is a symbol other than a space, or any background at all. A
/// space with a background is how a panel is filled, and skipping it would
/// leave the picture showing through the help text.
fn written(buffer: &Buffer) -> String {
    let mut out = String::new();
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        // Cells are addressed one at a time rather than in runs. A run would be
        // fewer bytes, but the overlay is a few hundred cells against a frame
        // of several million pixels - and a wrong run is a smear across the
        // picture, which is a poor trade for bytes nobody is short of.
        for x in area.left()..area.right() {
            let cell = &buffer[(x, y)];
            if cell.symbol() == " " && cell.bg == Color::Reset {
                continue;
            }
            // Rows and columns are 1-based in `CUP`, and the cursor has to be
            // put back afterwards or the next frame is placed from here.
            let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            out.push_str(&colours(cell.fg, cell.bg));
            out.push_str(cell.symbol());
            out.push_str("\x1b[0m");
        }
    }
    out
}

/// The SGR sequence that sets a cell's colours.
fn colours(fg: Color, bg: Color) -> String {
    let mut out = String::from("\x1b[");
    out.push_str(&code(fg, true));
    out.push(';');
    out.push_str(&code(bg, false));
    out.push('m');
    out
}

/// One colour as its SGR parameters.
///
/// Truecolour where the theme gave an exact colour, and the palette slot where
/// it named one - which is what keeps `terminal` and `mono` following whatever
/// the reader actually runs, exactly as the glyph surface does.
fn code(colour: Color, foreground: bool) -> String {
    let base = if foreground { 30 } else { 40 };
    let bright = if foreground { 90 } else { 100 };
    match colour {
        Color::Reset => format!("{}", base + 9),
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
        Color::Indexed(i) => format!("{};5;{i}", base + 8),
        Color::Black => format!("{base}"),
        Color::Red => format!("{}", base + 1),
        Color::Green => format!("{}", base + 2),
        Color::Yellow => format!("{}", base + 3),
        Color::Blue => format!("{}", base + 4),
        Color::Magenta => format!("{}", base + 5),
        Color::Cyan => format!("{}", base + 6),
        Color::Gray => format!("{}", base + 7),
        Color::DarkGray => format!("{bright}"),
        Color::LightRed => format!("{}", bright + 1),
        Color::LightGreen => format!("{}", bright + 2),
        Color::LightYellow => format!("{}", bright + 3),
        Color::LightBlue => format!("{}", bright + 4),
        Color::LightMagenta => format!("{}", bright + 5),
        Color::LightCyan => format!("{}", bright + 6),
        Color::White => format!("{}", bright + 7),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A widget that paints exactly the cells it is told to.
    struct Spots(Vec<(u16, u16, &'static str, Color)>);

    impl Widget for Spots {
        fn render(self, _area: Rect, buffer: &mut Buffer) {
            for (x, y, symbol, bg) in self.0 {
                buffer[(x, y)].set_symbol(symbol).set_bg(bg);
            }
        }
    }

    fn area() -> Rect {
        Rect::new(0, 0, 20, 5)
    }

    #[test]
    fn a_widget_that_draws_nothing_writes_nothing() {
        // Every frame with no status note and the help closed, which is almost
        // all of them. One stray space would blank the picture.
        let quiet = of(Spots(Vec::new()), area());
        assert!(quiet.is_empty(), "wrote {quiet:?} over the frame");
    }

    #[test]
    fn only_the_cells_a_widget_touched_are_written() {
        // The whole reason this exists: a written cell blanks the image beneath
        // it, so the overlay must cost exactly its own cells and no more.
        let escape = of(
            Spots(vec![(3, 1, "x", Color::Reset), (4, 1, "y", Color::Reset)]),
            area(),
        );
        assert_eq!(escape.matches('H').count(), 2, "moved more than twice");
        assert!(escape.contains("\x1b[2;4H"), "x is not at row 2, column 4");
        assert!(escape.contains("\x1b[2;5H"), "y is not at row 2, column 5");
    }

    #[test]
    fn a_space_with_a_background_is_a_cell_the_widget_meant() {
        // How a panel is filled. Skipping it would leave the bars showing
        // through the help text, which is the one place legibility beats the
        // picture.
        let filled = of(Spots(vec![(1, 1, " ", Color::Rgb(16, 20, 26))]), area());
        assert!(!filled.is_empty(), "the panel's own background was dropped");
        assert!(filled.contains("48;2;16;20;26"), "not that background");
    }

    #[test]
    fn a_named_colour_goes_out_as_a_palette_slot() {
        // What keeps `terminal` and `mono` following the reader's own colours:
        // the name reaches the terminal as a slot, not as an colour rav picked.
        let named = of(Spots(vec![(0, 0, "g", Color::Green)]), area());
        assert!(
            named.contains("42"),
            "green is not the palette's green: {named}"
        );
        assert!(
            !named.contains("48;2;"),
            "it was resolved to an exact colour"
        );
    }

    #[test]
    fn the_real_help_panel_costs_its_own_cells_and_no_more() {
        // The widget this exists for, rather than a stand-in. The panel is a
        // few hundred cells in the middle of a large grid; everything outside it
        // has to go unwritten or the picture is gone.
        use crate::ui::help::{Help, HelpRow};

        let rows = [
            HelpRow {
                key: "q",
                description: "quit",
                value: None,
            },
            HelpRow {
                key: "t",
                description: "theme",
                value: Some("rav".into()),
            },
        ];
        let grid = Rect::new(0, 0, 120, 40);
        let escape = of(
            Help {
                rows: &rows,
                title: "rav",
            },
            grid,
        );

        let touched = escape.matches("\x1b[").count() / 3; // move, colours, reset
        let whole_grid = usize::from(grid.width) * usize::from(grid.height);
        assert!(touched > 0, "the panel drew nothing");
        assert!(
            touched < whole_grid / 4,
            "{touched} of {whole_grid} cells written - the frame is gone",
        );
        assert!(
            escape.contains("48;2;16;20;26"),
            "the panel lost its own background, so the bars show through it",
        );
    }

    #[test]
    fn every_cell_puts_the_style_back() {
        // Left set, the next thing written to this terminal wears the overlay's
        // colours - including the shell, after rav exits.
        let escape = of(Spots(vec![(0, 0, "a", Color::Red)]), area());
        assert!(escape.ends_with("\x1b[0m"), "the style was left on");
    }
}
