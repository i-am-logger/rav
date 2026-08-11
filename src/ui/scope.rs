//! The oscilloscope view: the waveform itself, coloured by displacement.
//!
//! Drawn from the time domain rather than the FFT, and coloured
//! from the theme's five `scope` colours by displacement from the centre. The
//! first is the brightest, so the trace is brightest along the centre line and
//! fades as it swings away: a quiet passage is a bright flat line, a loud one
//! spreads into dimmer greys.
//!
//! The original clips at roughly ±0.5 of full scale - a 2x zoom - and rav keeps
//! that. Mapping the full ±1.0 across the height leaves ordinary music,
//! which peaks nearer ±0.2, as a barely-moving line through the middle. The gain
//! trim rides on top, so the same key that sizes the bars sizes the trace.

use crate::visual::Theme;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

/// The scope grid is 16 rows; the colour ladder is defined against it.
const VIRTUAL_ROWS: usize = 16;

const LINE: &str = "█";

/// Oscilloscope colour level for a row within the 16-row grid.
///
/// A V in *index* terms, and index 0 is the brightest colour - so the trace is
/// brightest along the centre line and fades towards the extremes. A quiet
/// passage reads as a bright flat line and a loud one spreads into dimmer greys.
fn oscilloscope_level(y: usize) -> usize {
    match y {
        y if y >= 14 => 4,
        y if y >= 12 => 3,
        y if y >= 10 => 2,
        y if y >= 8 => 1,
        y if y >= 6 => 0,
        y if y >= 4 => 1,
        y if y >= 2 => 2,
        _ => 3,
    }
}

/// The trace is clipped at ±0.5 of full scale, so the height covers half the
/// range - a 2x zoom.
const ZOOM: f32 = 2.0;

/// How the waveform is drawn between consecutive samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeStyle {
    /// One cell per column. Sparse and jumpy at low sample counts.
    Dots,
    /// Fill between consecutive samples so the trace is continuous.
    #[default]
    Lines,
}

pub struct Scope<'a> {
    /// Time-domain samples in `-1.0..=1.0`, oldest first.
    pub samples: &'a [f32],
    pub theme: &'a Theme,
    pub style: ScopeStyle,
    /// Linear gain from the trim, applied on top of `ZOOM`.
    pub gain: f32,
}

/// Row from the bottom that sample `value` maps to.
///
/// Truncating rather than rounding puts silence on the row *below* centre, which
/// is where the original's `round(16x) + 7` puts it on an even-height display. Both
/// extremes are still reachable.
fn row_of(value: f32, height: u16) -> u16 {
    let v = if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let last = height.saturating_sub(1) as f32;
    (((v + 1.0) / 2.0) * last) as u16
}

impl Widget for Scope<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || self.samples.is_empty() {
            return;
        }
        let height = area.height;
        let width = area.width;

        let mut previous: Option<u16> = None;
        for x in 0..width {
            // Spread the window across the available columns.
            let index = (x as usize * self.samples.len()) / width as usize;
            let sample = self.samples[index.min(self.samples.len() - 1)];
            // Clipping rather than rescaling: past full deflection the trace
            // flattens against the edge the way a real scope does.
            let row = row_of(sample * ZOOM * self.gain, height);

            // Fill the gap to the previous column so a steep slope stays joined
            // rather than breaking into disconnected dots.
            let (lo, hi) = match (self.style, previous) {
                (ScopeStyle::Lines, Some(prev)) => (prev.min(row), prev.max(row)),
                _ => (row, row),
            };
            previous = Some(row);

            for r in lo..=hi {
                // Colour by displacement from the centre, on the 16-row grid.
                let virtual_row = (r as usize * VIRTUAL_ROWS) / height.max(1) as usize;
                let level = oscilloscope_level(virtual_row.min(VIRTUAL_ROWS - 1));
                buf[(area.x + x, area.y + height - 1 - r)]
                    .set_symbol(LINE)
                    .set_fg(crate::ui::to_color(
                        self.theme.scope[level.min(self.theme.scope.len() - 1)],
                    ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(samples: &[f32], w: u16, h: u16, style: ScopeStyle) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let theme = Theme::default();
        Scope {
            samples,
            theme: &theme,
            style,
            gain: 1.0,
        }
        .render(area, &mut buf);
        buf
    }

    #[test]
    fn silence_draws_a_line_through_the_centre() {
        let buf = render(&[0.0; 16], 16, 16, ScopeStyle::Lines);
        // Row 7 from the bottom is row 8 from the top on a 16-row area.
        assert_eq!(buf[(0, 8)].symbol(), LINE);
        assert_eq!(buf[(0, 0)].symbol(), " ", "top stays clear");
        assert_eq!(buf[(0, 15)].symbol(), " ", "bottom stays clear");
    }

    #[test]
    fn full_scale_reaches_both_extremes() {
        assert_eq!(row_of(1.0, 16), 15);
        assert_eq!(row_of(-1.0, 16), 0);
        assert_eq!(row_of(0.0, 16), 7);
    }

    #[test]
    fn the_zoom_lets_ordinary_music_use_the_height() {
        // A trace at ±0.5 - loud for real material - must reach the extremes
        // rather than hugging the centre line.
        let wave: Vec<f32> = (0..32)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let buf = render(&wave, 32, 16, ScopeStyle::Dots);
        let rows: Vec<u16> = (0..32)
            .filter_map(|x| (0..16).find(|&y| buf[(x, y)].symbol() == LINE))
            .collect();
        assert!(rows.contains(&0), "never reached the top: {rows:?}");
        assert!(rows.contains(&15), "never reached the bottom: {rows:?}");
    }

    #[test]
    fn the_gain_trim_sizes_the_trace() {
        // The same key that sizes the bars must size this, or the two views
        // disagree about how loud the same audio is.
        let wave: Vec<f32> = (0..16)
            .map(|i| if i % 2 == 0 { 0.1 } else { -0.1 })
            .collect();
        let spread = |gain: f32| {
            let area = Rect::new(0, 0, 16, 16);
            let mut buf = Buffer::empty(area);
            let theme = Theme::default();
            Scope {
                samples: &wave,
                theme: &theme,
                style: ScopeStyle::Dots,
                gain,
            }
            .render(area, &mut buf);
            let rows: Vec<u16> = (0..16)
                .filter_map(|x| (0..16).find(|&y| buf[(x, y)].symbol() == LINE))
                .collect();
            rows.iter().max().unwrap() - rows.iter().min().unwrap()
        };
        assert!(spread(4.0) > spread(1.0), "more gain must swing further");
    }

    #[test]
    fn the_centre_of_the_trace_is_brighter_than_its_extremes() {
        // Displacement from the centre climbs the ladder, and the ladder starts
        // at white - so excursions fade rather than light up.
        let flat = render(&[0.0; 8], 8, 16, ScopeStyle::Lines);
        let loud = render(&[1.0; 8], 8, 16, ScopeStyle::Lines);
        let centre = flat[(0, 8)].fg;
        let extreme = loud[(0, 0)].fg;
        assert_ne!(centre, extreme, "extremes must not match the centre colour");
        assert_eq!(
            centre,
            crate::ui::to_color(Theme::default().scope[0]),
            "centre uses the brightest level"
        );
    }

    #[test]
    fn lines_style_joins_a_steep_slope() {
        // Alternating extremes: with Dots each column is a single cell; with
        // Lines the gap between them is filled.
        let samples = [-1.0f32, 1.0, -1.0, 1.0];
        let dots = render(&samples, 4, 16, ScopeStyle::Dots);
        let lines = render(&samples, 4, 16, ScopeStyle::Lines);
        let filled = |b: &Buffer, x: u16| (0..16).filter(|&y| b[(x, y)].symbol() == LINE).count();
        assert_eq!(filled(&dots, 1), 1, "dots draw one cell per column");
        assert!(filled(&lines, 1) > 8, "lines should bridge the jump");
    }

    #[test]
    fn degenerate_areas_and_inputs_do_not_panic() {
        for (w, h) in [(0u16, 0u16), (1, 1), (80, 1), (1, 40)] {
            let _ = render(&[0.5, -0.5], w, h, ScopeStyle::Lines);
        }
        let _ = render(&[], 10, 10, ScopeStyle::Lines);
        let _ = render(&[f32::NAN, f32::INFINITY], 10, 10, ScopeStyle::Lines);
    }

    #[test]
    fn more_samples_than_columns_are_subsampled_not_dropped() {
        let samples: Vec<f32> = (0..1024).map(|i| (i as f32 / 512.0) - 1.0).collect();
        let buf = render(&samples, 40, 16, ScopeStyle::Lines);
        // A rising ramp should end higher than it starts.
        let first = (0..16).find(|&y| buf[(0, y)].symbol() == LINE).unwrap();
        let last = (0..16).find(|&y| buf[(39, y)].symbol() == LINE).unwrap();
        assert!(last < first, "ramp should rise across the width");
    }
}
