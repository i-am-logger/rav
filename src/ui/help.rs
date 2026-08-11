//! The help overlay.
//!
//! Doubles as the settings readout: every line that has a current value shows
//! it, so this is the answer to "which frequency limit am I on" rather than
//! having to cycle a key and watch what changes.

use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

/// Panel colours. Only the overlay paints a background — the visualiser itself
/// leaves the terminal's own theme alone.
const PANEL_BG: Color = Color::Rgb(16, 20, 26);
const BORDER: Color = Color::Rgb(90, 100, 110);
const KEY: Color = Color::Rgb(239, 187, 49);
const TEXT: Color = Color::Rgb(222, 222, 222);
const VALUE: Color = Color::Rgb(41, 206, 16);

/// One row of the overlay: a key, what it does, and its current value.
pub struct HelpRow<'a> {
    pub key: &'a str,
    pub description: &'a str,
    pub value: Option<String>,
}

pub struct Help<'a> {
    pub rows: &'a [HelpRow<'a>],
    pub title: &'a str,
}

impl Help<'_> {
    /// Width of the key column, which every row is indented by.
    fn key_width(&self) -> usize {
        self.rows
            .iter()
            .map(|r| r.key.chars().count())
            .max()
            .unwrap_or(0)
    }

    /// Width the panel wants, from its widest row.
    ///
    /// This sizes against the *shared* key column, not each row's own key,
    /// because that is what layout indents by. Measuring per-row leaves any row
    /// with a short key and a long description too narrow, and its value is then
    /// dropped without a trace.
    fn desired_width(&self) -> u16 {
        let key = self.key_width();
        let widest = self
            .rows
            .iter()
            .map(|r| {
                key + r.description.chars().count()
                    + r.value.as_ref().map_or(0, |v| v.chars().count() + 2)
            })
            .max()
            .unwrap_or(0);
        (widest as u16 + 8).max(self.title.chars().count() as u16 + 6)
    }

    /// Centre the panel, shrinking it if the terminal is too small.
    fn panel(&self, area: Rect) -> Rect {
        let width = self.desired_width().min(area.width);
        let height = (self.rows.len() as u16 + 4).min(area.height);
        Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        }
    }
}

/// Write `text` at `(x, y)`, clipped to `bounds`.
fn write(buf: &mut Buffer, bounds: Rect, x: u16, y: u16, text: &str, fg: Color) {
    if y < bounds.y || y >= bounds.y + bounds.height {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        let cx = x + i as u16;
        if cx >= bounds.x + bounds.width {
            break;
        }
        buf[(cx, y)].set_symbol(&ch.to_string()).set_fg(fg);
    }
}

impl Widget for Help<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let panel = self.panel(area);
        if panel.width < 4 || panel.height < 3 {
            return;
        }

        // Fill and frame. The overlay is the one place a background is painted,
        // because unreadable help is worse than a theme mismatch.
        for y in panel.y..panel.y + panel.height {
            for x in panel.x..panel.x + panel.width {
                buf[(x, y)].set_symbol(" ").set_bg(PANEL_BG).set_fg(TEXT);
            }
        }
        let right = panel.x + panel.width - 1;
        let bottom = panel.y + panel.height - 1;
        for x in panel.x..=right {
            buf[(x, panel.y)].set_symbol("─").set_fg(BORDER);
            buf[(x, bottom)].set_symbol("─").set_fg(BORDER);
        }
        for y in panel.y..=bottom {
            buf[(panel.x, y)].set_symbol("│").set_fg(BORDER);
            buf[(right, y)].set_symbol("│").set_fg(BORDER);
        }
        buf[(panel.x, panel.y)].set_symbol("╭").set_fg(BORDER);
        buf[(right, panel.y)].set_symbol("╮").set_fg(BORDER);
        buf[(panel.x, bottom)].set_symbol("╰").set_fg(BORDER);
        buf[(right, bottom)].set_symbol("╯").set_fg(BORDER);

        let title = format!(" {} ", self.title);
        let tx = panel.x + (panel.width.saturating_sub(title.chars().count() as u16)) / 2;
        write(buf, panel, tx, panel.y, &title, KEY);

        // Key column is fixed so the descriptions line up.
        let key_width = self.key_width() as u16;

        for (i, row) in self.rows.iter().enumerate() {
            let y = panel.y + 2 + i as u16;
            if y >= bottom {
                break;
            }
            write(buf, panel, panel.x + 2, y, row.key, KEY);
            let dx = panel.x + 2 + key_width + 2;
            write(buf, panel, dx, y, row.description, TEXT);
            if let Some(value) = &row.value {
                let vx = right.saturating_sub(value.chars().count() as u16 + 1);
                if vx > dx + row.description.chars().count() as u16 {
                    write(buf, panel, vx, y, value, VALUE);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<HelpRow<'static>> {
        vec![
            HelpRow {
                key: "q",
                description: "quit",
                value: None,
            },
            HelpRow {
                key: "r",
                description: "frequency range",
                value: Some("up to 16 kHz".into()),
            },
        ]
    }

    fn render(w: u16, h: u16) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let rows = rows();
        Help {
            rows: &rows,
            title: "rav",
        }
        .render(area, &mut buf);
        buf
    }

    fn text_of(buf: &Buffer) -> String {
        let a = *buf.area();
        (0..a.height)
            .map(|y| {
                (0..a.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn shows_keys_descriptions_and_current_values() {
        let out = text_of(&render(60, 12));
        assert!(out.contains("quit"), "missing a description:\n{out}");
        assert!(out.contains("frequency range"));
        // The whole point: the active setting is readable without cycling it.
        assert!(out.contains("up to 16 kHz"), "missing the value:\n{out}");
    }

    #[test]
    fn the_panel_is_centred_and_framed() {
        let buf = render(60, 12);
        let out = text_of(&buf);
        assert!(out.contains('╭') && out.contains('╯'), "no frame:\n{out}");
        // Centred: the leftmost column should be untouched background.
        assert_eq!(buf[(0, 0)].symbol(), " ");
    }

    #[test]
    fn it_shrinks_rather_than_overflowing_a_small_terminal() {
        for (w, h) in [(0u16, 0u16), (1, 1), (10, 3), (20, 5), (200, 60)] {
            let buf = render(w, h);
            assert_eq!(buf.area().width, w, "must not resize the area");
        }
    }

    #[test]
    fn every_row_shows_its_value_at_a_normal_terminal_size() {
        // Sizing and layout have to agree on the key column. Measuring each row
        // against its own key while laying out against the widest one leaves
        // short-key rows too narrow, and they drop their value silently.
        let rows = vec![
            HelpRow {
                key: "space",
                description: "switch visualisation",
                value: Some("analyser".into()),
            },
            HelpRow {
                key: "r",
                description: "frequency range",
                value: Some("up to 16 kHz".into()),
            },
            HelpRow {
                key: "up/down",
                description: "gain (0 resets)",
                value: Some("+0 dB".into()),
            },
        ];
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        Help {
            rows: &rows,
            title: "rav",
        }
        .render(area, &mut buf);
        let out = text_of(&buf);
        for want in ["up to 16 kHz", "analyser", "+0 dB"] {
            assert!(out.contains(want), "missing {want:?} in:\n{out}");
        }
    }

    #[test]
    fn a_value_is_dropped_rather_than_overlapping_its_description() {
        // Narrow panel: the description wins, the value is omitted, and nothing
        // is drawn on top of anything else.
        let out = text_of(&render(24, 10));
        assert!(!out.contains("up to 16 kHz"));
    }
}
