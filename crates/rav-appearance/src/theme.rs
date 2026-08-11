//! Themes: the colours the display is drawn in, as data.
//!
//! A theme is a TOML file. Everything rav ships is one of these — nothing about
//! the palette lives in code — so contributing a theme means writing a file, not
//! touching Rust. `docs/themes.md` is the format's documentation.
//!
//! Colours are either `"#rrggbb"`, which looks the same everywhere, or one of
//! the sixteen ANSI names, which the terminal paints from the active theme. A
//! theme can mix them freely: that choice is the difference between reproducing
//! a specific look and following whatever the user already runs.
//!
//! # Two shapes of the same thing
//!
//! [`StaticTheme`] is what `build.rs` generates from the bundled files: plain
//! data in read-only memory, no allocator, no parser. A target with neither can
//! still have every theme rav ships.
//!
//! [`Theme`] owns its colours, because a theme read from a user's disk at
//! runtime has to come from somewhere. Reading one needs a TOML parser and a
//! filesystem, so that lives in the binary rather than here.

use crate::ink::Ink;
use crate::palette::Palette;
use crate::ramp::Ramp;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Stops in the analyser ramp, and in a per-stop backdrop.
pub const STOPS: usize = 16;
/// Levels in the oscilloscope ladder.
pub const SCOPE_LEVELS: usize = 5;

include!(concat!(env!("OUT_DIR"), "/built_in.rs"));

/// A theme as `build.rs` emits it: fixed-size, const-constructible, no allocator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticTheme {
    pub name: &'static str,
    pub bars: [Ink; STOPS],
    pub grid: [Ink; STOPS],
    pub peak: Ink,
    pub scope: [Ink; SCOPE_LEVELS],
    pub darken: Option<f32>,
}

/// A theme that owns its colours - one read from disk, or a built-in taken up
/// into allocated form.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    /// Analyser ramp, foot of the bar first.
    pub bars: Vec<Ink>,
    /// Backdrop per stop, aligned with `bars`.
    pub grid: Vec<Ink>,
    pub peak: Ink,
    /// Oscilloscope ladder, centre of the trace first.
    pub scope: Vec<Ink>,
    /// How much brightness the backdrop keeps, when the theme asked to darken it.
    ///
    /// Applied at render time rather than here, because darkening a colour the
    /// theme only *named* needs the terminal's actual palette - see [`Palette`].
    pub darken: Option<f32>,
}

impl From<&StaticTheme> for Theme {
    fn from(fixed: &StaticTheme) -> Self {
        Self {
            name: fixed.name.to_string(),
            bars: fixed.bars.to_vec(),
            grid: fixed.grid.to_vec(),
            peak: fixed.peak,
            scope: fixed.scope.to_vec(),
            darken: fixed.darken,
        }
    }
}

impl Default for Theme {
    /// The first bundled theme. A const rather than a parse, so this cannot fail
    /// and needs no TOML reader.
    fn default() -> Self {
        Self::from(BUILT_IN[0].1)
    }
}

impl Theme {
    /// Names of the compiled-in themes, in the order `t` cycles them.
    pub fn built_in_names() -> impl Iterator<Item = &'static str> {
        BUILT_IN.iter().map(|(name, _)| *name)
    }

    /// One of the compiled-in themes by name.
    pub fn built_in(name: &str) -> Option<Self> {
        BUILT_IN
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, theme)| Self::from(*theme))
    }

    /// The lit ramp, for a surface that draws pixels.
    pub fn bar_ramp(&self, palette: &Palette) -> Ramp {
        Ramp::new(self.bars.iter().map(|&c| palette.resolve(c)).collect())
    }

    /// The backdrop ramp, with `darken` applied.
    ///
    /// Dims through the same [`Ink::dimmed`] the glyph renderer uses, so the two
    /// surfaces cannot disagree about what a theme asked for - they had two
    /// implementations of this and the difference was invisible. Only the last
    /// step differs, and legitimately: a pixel surface has no terminal to defer
    /// to, so it resolves a name to the standard value where the glyph renderer
    /// hands the name onward.
    pub fn grid_ramp(&self, palette: &Palette) -> Ramp {
        Ramp::new(
            self.grid
                .iter()
                .map(|&ink| palette.resolve(ink.dimmed(self.darken, palette.rgb(ink))))
                .collect(),
        )
    }

    /// The peak cap's colour, for a surface that draws pixels.
    pub fn cap_colour(&self, palette: &Palette) -> crate::ink::Colour {
        palette.resolve(self.peak)
    }

    /// Whether rendering this theme needs the terminal's palette read.
    ///
    /// Only a theme that both asks to darken *and* names a colour rather than
    /// spelling it out: there is nothing to look up otherwise, and asking is not
    /// free - it writes escape sequences and reads the replies off the terminal.
    pub fn needs_terminal_palette(&self) -> bool {
        self.darken.is_some() && self.grid.iter().any(|c| !c.is_exact())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_theme_is_a_const() {
        // The point of generating them: no parser runs, so none of this can
        // fail at startup on a target that has no parser to run.
        assert!(BUILT_IN.len() >= 4, "rav ships four themes");
        for (name, theme) in BUILT_IN.iter() {
            assert_eq!(theme.name, *name, "the file's name field disagrees");
            assert_eq!(theme.bars.len(), STOPS);
            assert_eq!(theme.grid.len(), STOPS);
            assert_eq!(theme.scope.len(), SCOPE_LEVELS);
        }
    }

    #[test]
    fn the_default_is_rav_and_the_cycle_order_is_the_curated_one() {
        // Named rather than "whatever is first", because generating these from
        // a directory listing made `mono` the default and reshuffled the `t`
        // cycle - and a test asserting only self-consistency passed anyway.
        assert_eq!(Theme::default().name, "rav");
        let order: Vec<&str> = Theme::built_in_names().collect();
        assert_eq!(order, ["rav", "winamp", "terminal", "mono"]);
    }

    #[test]
    fn an_unknown_theme_is_not_invented() {
        assert!(Theme::built_in("puce").is_none());
    }

    #[test]
    fn mono_still_defers_its_backdrop_to_the_terminal() {
        // The regression guard, carried across with the type: mono declares no
        // darken and a grid of bright-black, and folding "no instruction" into
        // "keep all of it" would resolve that against a startup snapshot.
        let mono = Theme::built_in("mono").expect("mono is bundled");
        assert_eq!(mono.darken, None);
        let palette = Palette::answering([(9, 9, 9); 16]);
        for &ink in &mono.grid {
            assert!(!ink.dimmed(mono.darken, palette.rgb(ink)).is_exact());
        }
    }
}
