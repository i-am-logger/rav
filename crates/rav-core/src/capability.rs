//! What a visualisation needs, and what a surface can offer.
//!
//! rav runs on anything from a single bar of four LEDs to a 16K display, and
//! not every visualisation survives that. An oscilloscope spends its horizontal
//! axis on *time*, so a one-column strip has nowhere to put the waveform - that
//! is a missing dimension rather than a shortage of resolution, and no amount of
//! shrinking fixes it. A VU meter needs one column and two rungs and is happy.
//!
//! So a surface reports what it has, a visualisation states what it needs, and
//! the ones that cannot say anything useful should never be offered. That makes
//! the small end a first-class target rather than a degraded one: on a
//! four-light bar the answer is "this shows a VU meter", not "this shows a
//! broken spectrum".
//!
//! Composition falls out of the same check. Splitting a screen gives each half
//! its own, smaller [`Capabilities`], so a visualisation that fits alone may not
//! fit beside another - which is the honest answer rather than a special case.
//!
//! # Nothing consults this yet
//!
//! `space` cycles the analyser and the oscilloscope without asking either of
//! them anything, and no surface is refused a visualisation. This module is the
//! rule written down and tested, not the rule being applied - and on a terminal
//! the difference is invisible, because a terminal always has columns enough
//! for both.
//!
//! It becomes load-bearing on the first surface that cannot show everything: a
//! four-light bar, where offering an oscilloscope means offering a waveform with
//! nowhere to put time. Until then this describes what should happen rather than
//! what does.

use crate::units::Cells;

/// What a surface can actually show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Independent horizontal positions: bars, LEDs across, terminal columns.
    pub columns: usize,
    /// Vertical resolution within one column.
    pub rungs: usize,
    /// Distinguishable levels a rung can take. `1` is on-or-off; a single-colour
    /// LED with PWM has as many as its driver has steps; a truecolour display
    /// has more than anyone can perceive.
    pub shades: usize,
    /// Input channels the surface can lay out separately. `1` folds to mono.
    pub channels: usize,
    /// Frames of history the build can afford to keep. Zero on a target with no
    /// allocator, which is what rules out a spectrogram there.
    pub history: usize,
    /// Whether things drawn later can lie *over* what is already there.
    ///
    /// False for a terminal glyph grid: a cell holds one symbol and one
    /// background, which is why a cap cannot sit over the backdrop and why a
    /// written cell blanks a kitty image. True for anything that composites.
    pub layers: bool,
}

impl Capabilities {
    /// A terminal, whose grid is measured in cells.
    ///
    /// Eight shades because the block glyphs divide a cell into eighths, and no
    /// layering because one cell holds one symbol.
    pub fn terminal(columns: Cells, rows: Cells) -> Self {
        Self {
            columns: usize::from(columns.count()),
            rungs: usize::from(rows.count()),
            shades: 8,
            channels: 1,
            history: usize::MAX,
            layers: false,
        }
    }

    /// The area left after taking `columns` of it, for a split layout.
    pub fn narrowed_to(self, columns: usize) -> Self {
        Self {
            columns: columns.min(self.columns),
            ..self
        }
    }

    /// The area left after taking `rungs` of it, for a stacked layout.
    pub fn shortened_to(self, rungs: usize) -> Self {
        Self {
            rungs: rungs.min(self.rungs),
            ..self
        }
    }
}

/// What a visualisation needs before it can say anything useful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirements {
    pub columns: usize,
    pub rungs: usize,
    pub shades: usize,
    pub channels: usize,
    pub history: usize,
}

impl Requirements {
    /// The floor: one column, one rung, on or off, mono, no memory. What a
    /// peak-or-not indicator needs, and the least anything can ask for.
    pub const MINIMAL: Self = Self {
        columns: 1,
        rungs: 1,
        shades: 1,
        channels: 1,
        history: 0,
    };

    pub const fn columns(self, columns: usize) -> Self {
        Self { columns, ..self }
    }

    pub const fn rungs(self, rungs: usize) -> Self {
        Self { rungs, ..self }
    }

    pub const fn shades(self, shades: usize) -> Self {
        Self { shades, ..self }
    }

    pub const fn stereo(self) -> Self {
        Self {
            channels: 2,
            ..self
        }
    }

    pub const fn history(self, history: usize) -> Self {
        Self { history, ..self }
    }

    /// Whether a surface can show this at all.
    pub const fn met_by(&self, surface: &Capabilities) -> bool {
        self.columns <= surface.columns
            && self.rungs <= surface.rungs
            && self.shades <= surface.shades
            && self.channels <= surface.channels
            && self.history <= surface.history
    }

    /// What is missing, so a surface can say *why* rather than just refusing.
    ///
    /// The first shortfall only. A display that is short of three things is not
    /// three problems to a user - it is the wrong display for this.
    pub fn shortfall(&self, surface: &Capabilities) -> Option<Shortfall> {
        if self.columns > surface.columns {
            Some(Shortfall::Columns {
                needs: self.columns,
                has: surface.columns,
            })
        } else if self.rungs > surface.rungs {
            Some(Shortfall::Rungs {
                needs: self.rungs,
                has: surface.rungs,
            })
        } else if self.shades > surface.shades {
            Some(Shortfall::Shades {
                needs: self.shades,
                has: surface.shades,
            })
        } else if self.channels > surface.channels {
            Some(Shortfall::Channels {
                needs: self.channels,
                has: surface.channels,
            })
        } else if self.history > surface.history {
            Some(Shortfall::History {
                needs: self.history,
                has: surface.history,
            })
        } else {
            None
        }
    }
}

/// Why a surface cannot show a visualisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortfall {
    Columns { needs: usize, has: usize },
    Rungs { needs: usize, has: usize },
    Shades { needs: usize, has: usize },
    Channels { needs: usize, has: usize },
    History { needs: usize, has: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single bar of five single-colour LEDs, the smallest target rav aims at.
    /// Brightness gives it shades; one column gives it no time axis.
    fn led_bar() -> Capabilities {
        Capabilities {
            columns: 1,
            rungs: 5,
            shades: 32,
            channels: 1,
            history: 0,
            layers: true,
        }
    }

    /// Eight by eight, the common hobby matrix.
    fn led_matrix() -> Capabilities {
        Capabilities {
            columns: 8,
            rungs: 8,
            shades: 32,
            channels: 1,
            history: 0,
            layers: true,
        }
    }

    // The visualisations, as they would declare themselves.
    fn vu() -> Requirements {
        Requirements::MINIMAL.rungs(2)
    }
    fn spectrum() -> Requirements {
        Requirements::MINIMAL.columns(2).rungs(2)
    }
    fn oscilloscope() -> Requirements {
        Requirements::MINIMAL.columns(16).rungs(3)
    }
    fn spectrogram() -> Requirements {
        Requirements::MINIMAL
            .columns(16)
            .rungs(16)
            .shades(8)
            .history(64)
    }
    fn goniometer() -> Requirements {
        Requirements::MINIMAL.columns(16).rungs(16).stereo()
    }

    #[test]
    fn a_four_light_bar_shows_a_vu_meter_and_nothing_else() {
        // The floor of rav's range, and the reason the requirements exist. An
        // oscilloscope is not merely coarse here - it has no axis to put time
        // on, and shrinking cannot supply one.
        let bar = led_bar();
        assert!(vu().met_by(&bar), "a VU meter is what an LED bar is");
        assert!(!spectrum().met_by(&bar), "one column is not a spectrum");
        assert!(!oscilloscope().met_by(&bar));
        assert!(!spectrogram().met_by(&bar));
        assert!(!goniometer().met_by(&bar));
    }

    #[test]
    fn the_shortfall_says_which_dimension_is_missing() {
        // So a surface can explain itself rather than silently offering less.
        assert_eq!(
            oscilloscope().shortfall(&led_bar()),
            Some(Shortfall::Columns { needs: 16, has: 1 })
        );
        assert_eq!(
            goniometer().shortfall(&led_matrix()),
            Some(Shortfall::Columns { needs: 16, has: 8 }),
            "the first shortfall only - it is the wrong display, not three faults"
        );
        assert_eq!(vu().shortfall(&led_bar()), None);
    }

    #[test]
    fn a_matrix_gains_the_spectrum_but_still_not_the_history() {
        // Eight columns is a spectrum; no allocator is still no spectrogram.
        let matrix = led_matrix();
        assert!(spectrum().met_by(&matrix));
        assert!(vu().met_by(&matrix));
        assert!(
            !oscilloscope().met_by(&matrix),
            "eight columns is not a wave"
        );
        // Short of columns *before* it is short of history, and the first
        // shortfall is the one reported.
        assert_eq!(
            spectrogram().shortfall(&matrix),
            Some(Shortfall::Columns { needs: 16, has: 8 })
        );

        // Given the columns, history is what an allocator-free target still
        // cannot supply - a scrollback buffer has to live somewhere.
        let wide_but_forgetful = Capabilities {
            columns: 64,
            rungs: 32,
            ..matrix
        };
        assert_eq!(
            spectrogram().shortfall(&wide_but_forgetful),
            Some(Shortfall::History { needs: 64, has: 0 })
        );
    }

    #[test]
    fn a_terminal_shows_everything_that_does_not_need_stereo() {
        let terminal = Capabilities::terminal(Cells(80), Cells(24));
        assert!(vu().met_by(&terminal));
        assert!(spectrum().met_by(&terminal));
        assert!(oscilloscope().met_by(&terminal));
        assert!(spectrogram().met_by(&terminal));
        // rav folds to mono at the source today, so nothing offers two channels.
        assert_eq!(
            goniometer().shortfall(&terminal),
            Some(Shortfall::Channels { needs: 2, has: 1 })
        );
    }

    #[test]
    fn splitting_a_screen_can_take_a_visualisation_away() {
        // Composition checked against the *region*, not the screen. A wave fits
        // an 80-column terminal and not a 10-column panel of one, which is the
        // honest answer rather than a special case for small areas.
        let whole = Capabilities::terminal(Cells(80), Cells(24));
        assert!(oscilloscope().met_by(&whole));
        assert!(oscilloscope().met_by(&whole.narrowed_to(40)));
        assert!(!oscilloscope().met_by(&whole.narrowed_to(10)));
        // Stacking costs rungs instead, and a VU meter survives being squeezed
        // to two where a spectrogram does not.
        assert!(vu().met_by(&whole.shortened_to(2)));
        assert!(!spectrogram().met_by(&whole.shortened_to(2)));
    }

    #[test]
    fn a_glyph_grid_cannot_layer_and_a_pixel_surface_can() {
        // The measured behaviour behind #65 and the kitty occlusion result: a
        // cell holds one symbol and one background, so a cap drawn over the
        // backdrop replaces it and a written cell blanks an image.
        assert!(!Capabilities::terminal(Cells(80), Cells(24)).layers);
        assert!(led_bar().layers);
    }

    #[test]
    fn the_minimal_requirement_is_met_by_anything_that_shows_at_all() {
        for surface in [
            led_bar(),
            led_matrix(),
            Capabilities::terminal(Cells(80), Cells(24)),
        ] {
            assert!(Requirements::MINIMAL.met_by(&surface));
        }
        // Even one light that is only ever on or off.
        let single = Capabilities {
            columns: 1,
            rungs: 1,
            shades: 1,
            channels: 1,
            history: 0,
            layers: false,
        };
        assert!(Requirements::MINIMAL.met_by(&single));
        assert!(!vu().met_by(&single), "a VU meter needs somewhere to move");
    }
}
