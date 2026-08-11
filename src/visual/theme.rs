//! Themes: the colours the display is drawn in, as data.
//!
//! A theme is a TOML file. Everything rav ships is one of these — nothing about
//! the palette lives in code — so contributing a theme means writing a file, not
//! touching Rust. `docs/themes.md` is the format's documentation; this module is
//! its implementation.
//!
//! Colours are either `"#rrggbb"`, which looks the same everywhere, or one of
//! the sixteen ANSI names, which the terminal paints from the active theme. A
//! theme can mix them freely: that choice is the difference between reproducing a
//! specific look and following whatever the user already runs.

use crate::render::{Colour, Ink, Ramp};
use crate::visual::Palette;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Stops in the analyser ramp, and in a per-stop backdrop.
pub const STOPS: usize = 16;
/// Levels in the oscilloscope ladder.
pub const SCOPE_LEVELS: usize = 5;

/// The themes rav ships, compiled in so the binary needs nothing on disk. First
/// is the default, and the order is the one `s` cycles.
///
/// `rav.toml` and `winamp.toml` carry their own attribution blocks - that is
/// where the MIT notice for the transcribed ramp lives, there being no vendored
/// file to put it alongside.
const BUILT_IN: [(&str, &str); 4] = [
    ("rav", include_str!("../../themes/rav.toml")),
    ("winamp", include_str!("../../themes/winamp.toml")),
    ("terminal", include_str!("../../themes/terminal.toml")),
    ("mono", include_str!("../../themes/mono.toml")),
];

/// A parsed theme: every colour the display needs, resolved and length-checked.
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
    /// theme only *named* needs the terminal's actual palette - see
    /// [`crate::visual::Palette`].
    pub darken: Option<f32>,
}

impl Default for Theme {
    fn default() -> Self {
        // Validated by `every_built_in_theme_parses`, so a failure here means the
        // build embedded something unexpected.
        Self::built_in("rav")
            .expect("rav is built in")
            .expect("the bundled rav theme must parse")
    }
}

impl Theme {
    /// Names of the compiled-in themes, in the order `s` cycles them.
    pub fn built_in_names() -> impl Iterator<Item = &'static str> {
        BUILT_IN.iter().map(|(name, _)| *name)
    }

    /// The lit ramp, for a surface that draws pixels.
    ///
    /// The conversion lives here rather than in `render` so that crate keeps
    /// knowing nothing about terminals - a [`Palette`] is an OSC 4 reader, and
    /// the renderer is meant to survive being made `no_std`.
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
    pub fn cap_colour(&self, palette: &Palette) -> Colour {
        palette.resolve(self.peak)
    }

    /// One of the compiled-in themes by name.
    pub fn built_in(name: &str) -> Option<Result<Self>> {
        BUILT_IN
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(n, text)| Self::parse(text).with_context(|| format!("built-in theme '{n}'")))
    }

    /// Load a theme: a built-in name, or a `.toml` file on disk.
    ///
    /// Built-ins win over a file of the same name, so `--theme mono` cannot be
    /// silently shadowed by a `themes/mono.toml` that happens to be lying around.
    pub fn load(spec: &str) -> Result<Self> {
        if let Some(built_in) = Self::built_in(spec) {
            return built_in;
        }
        let path = Self::resolve(spec)
            .with_context(|| format!("no theme '{spec}': not built in, and no such file"))?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Turn a theme spec into the file it names, if one exists.
    ///
    /// A bare name is looked up in `themes/` under the current directory - which
    /// is the repo checkout during development - and then in
    /// `<config dir>/rav/themes/`, which is where an installed rav finds one.
    fn resolve(spec: &str) -> Option<PathBuf> {
        let direct = Path::new(spec);
        if direct.is_file() {
            return Some(direct.to_path_buf());
        }
        let mut roots = vec![PathBuf::from("themes")];
        if let Some(dir) = dirs::config_dir() {
            roots.push(dir.join("rav").join("themes"));
        }
        roots.into_iter().find_map(|root| {
            let named = root.join(format!("{spec}.toml"));
            named.is_file().then_some(named)
        })
    }

    /// Whether rendering this theme needs the terminal's palette read.
    ///
    /// Only a theme that both asks to darken *and* names a colour rather than
    /// spelling it out: there is nothing to look up otherwise, and asking is not
    /// free - it writes escape sequences and reads the replies off the terminal.
    pub fn needs_terminal_palette(&self) -> bool {
        self.darken.is_some() && self.grid.iter().any(|c| !c.is_exact())
    }

    /// Parse a theme file.
    pub fn parse(text: &str) -> Result<Self> {
        let file: File = toml::from_str(text).context("not a valid theme file")?;

        // A ramp may be a single colour or a full ladder; a single one means
        // "this colour all the way up", which is how `mono` is written.
        let bars = file.colors.bars.expand(STOPS).context("colors.bars")?;
        let grid = file.colors.grid.expand(STOPS).context("colors.grid")?;
        let scope = file
            .colors
            .scope
            .expand(SCOPE_LEVELS)
            .context("colors.scope")?;
        let peak = parse_color(&file.colors.peak).context("colors.peak")?;

        let theme = Self {
            name: file.name,
            bars,
            grid,
            peak,
            scope,
            darken: file.darken.floor(),
        };
        Ok(theme)
    }
}

/// The on-disk shape. `about` is documentation for humans and is not read back.
#[derive(Deserialize)]
struct File {
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    description: String,
    #[serde(default)]
    darken: Darken,
    #[serde(default)]
    colors: Colors,
}

/// Deepen the shadow end of a theme's palette.
///
/// `true` for the default strength, or a number in `0.0..=1.0` for how far the
/// darkest colour is pushed towards black - `0.25` means it keeps a quarter of
/// its brightness. Themes written for a 16-pixel panel on a CRT tend to read too
/// bright as full-height terminal columns, and their backdrop worst of all,
/// because it is now a large area rather than a hairline of dots.
#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(untagged)]
enum Darken {
    Off(bool),
    By(f32),
}

impl Default for Darken {
    fn default() -> Self {
        Darken::Off(false)
    }
}

/// How much brightness the darkest colour keeps when `darken = true`.
const DEFAULT_DARKEN: f32 = 0.25;

impl Darken {
    /// The floor factor, or `None` when the theme is left alone.
    fn floor(self) -> Option<f32> {
        match self {
            Darken::Off(false) => None,
            Darken::Off(true) => Some(DEFAULT_DARKEN),
            Darken::By(f) => Some(f.clamp(0.0, 1.0)),
        }
    }
}

#[derive(Deserialize, Default)]
struct Colors {
    bars: Ladder,
    grid: Ladder,
    peak: String,
    scope: Ladder,
}

/// One colour, or a full ladder of them.
#[derive(Deserialize)]
#[serde(untagged)]
enum Ladder {
    One(String),
    Many(Vec<String>),
}

impl Default for Ladder {
    fn default() -> Self {
        Ladder::Many(Vec::new())
    }
}

impl Ladder {
    /// Resolve to exactly `len` colours.
    ///
    /// A single colour repeats; a ladder must already be the right length. Silently
    /// padding a short one would put an arbitrary colour at the top of the ramp,
    /// which is exactly the sort of thing that looks like a rendering bug later.
    fn expand(&self, len: usize) -> Result<Vec<Ink>> {
        match self {
            Ladder::One(name) => Ok(vec![parse_color(name)?; len]),
            Ladder::Many(names) => {
                if names.len() != len {
                    bail!("needs 1 or {len} colours, found {}", names.len());
                }
                names.iter().map(|n| parse_color(n)).collect()
            }
        }
    }
}

/// `#rrggbb`, or one of the sixteen ANSI names.
fn parse_color(text: &str) -> Result<Ink> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() != 6 {
            bail!("{text:?}: a hex colour is #rrggbb");
        }
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16);
        let (r, g, b) = (channel(0), channel(2), channel(4));
        return match (r, g, b) {
            (Ok(r), Ok(g), Ok(b)) => Ok(Ink::Rgb(r, g, b)),
            _ => bail!("{text:?}: a hex colour is #rrggbb"),
        };
    }
    // A name stays a name. Resolving it here would replace the user's green
    // with one rav picked, which is the opposite of what the `terminal` theme
    // is for - each surface settles it in the way that surface can.
    Ink::from_name(text)
        .ok_or_else(|| anyhow::anyhow!("{text:?} is not a colour: use #rrggbb or an ANSI name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Length;

    /// A theme with `bars`, `grid` and `peak` set to what the test needs.
    fn themed(grid: &str, darken: Option<f32>) -> Theme {
        Theme {
            grid: vec![parse_color(grid).expect("test colour"); STOPS],
            darken,
            ..Theme::default()
        }
    }

    #[test]
    fn a_pixel_ramp_carries_one_stop_per_theme_colour() {
        let theme = Theme::default();
        let palette = Palette::default();
        assert_eq!(theme.bar_ramp(&palette).len(), STOPS);
        assert_eq!(theme.grid_ramp(&palette).len(), STOPS);
    }

    /// Sixteen slots a terminal might report, distinct enough to tell apart.
    fn answering() -> Palette {
        let mut slots = [(0u8, 0u8, 0u8); 16];
        for (i, slot) in slots.iter_mut().enumerate() {
            *slot = (10 * i as u8, 200 - 5 * i as u8, 40);
        }
        Palette::answering(slots)
    }

    #[test]
    fn darken_scales_a_colour_the_terminal_answered_for() {
        let palette = answering();
        let plain = themed("green", None).grid_ramp(&palette);
        let dimmed = themed("green", Some(0.25)).grid_ramp(&palette);
        assert_ne!(plain, dimmed, "darken must reach an answered colour");

        // Scaled, not replaced: the hue survives, there is just less of it.
        let lit = plain.at(Length::NONE, Length(1.0));
        let dim = dimmed.at(Length::NONE, Length(1.0));
        assert!(
            dim.green < lit.green,
            "{dim:?} should be dimmer than {lit:?}"
        );
        assert!(dim.red <= lit.red && dim.blue <= lit.blue);
    }

    #[test]
    fn a_theme_that_asks_for_no_darken_keeps_its_ink_named() {
        // The mono regression this exists to prevent. `mono` declares no
        // `darken` and a `grid` of `bright-black`, and it is one of the two
        // themes whose whole purpose is to follow the user's palette.
        //
        // Folding "no instruction" into `Some(1.0)` would resolve that ink
        // against whatever the palette said at startup, freezing it - and it is
        // reachable, because cycling `t` reaches mono *through* `terminal`,
        // which is the theme that triggers the OSC 4 query. So mono would render
        // one way with `--theme mono` and another after three keypresses.
        //
        // The palette here *answers*, which is the case a default palette cannot
        // test: with nothing to resolve against, any implementation passes.
        let palette = answering();
        let mono = Theme::built_in("mono").expect("built in").expect("parses");
        assert_eq!(mono.darken, None, "mono asks for no darkening");
        for &ink in &mono.grid {
            assert!(
                !ink.dimmed(mono.darken, palette.rgb(ink)).is_exact(),
                "a theme that said nothing must keep {ink:?} deferred to the terminal"
            );
        }
    }

    #[test]
    fn both_surfaces_dim_by_the_same_rule() {
        // They had two implementations of darken and the difference was
        // invisible. Only the final resolution may differ now: the glyph
        // renderer hands a name onward, a pixel surface resolves it.
        // A real theme, whose sixteen grid stops differ from each other - a
        // single repeated colour would pass whatever the ordering, which is no
        // test at all.
        let palette = answering();
        let theme = Theme::built_in("terminal")
            .expect("built in")
            .expect("parses");
        assert!(
            theme.grid.windows(2).any(|pair| pair[0] != pair[1]),
            "the fixture must have distinct stops or this proves nothing"
        );

        // At exactly one row per stop the terminal takes them in order, so row r
        // from the floor is stop r, and the two lists line up directly.
        let glyph = crate::ui::analyzer::grid_colors(STOPS as u16, &theme, &palette);
        let pixels = theme.grid_ramp(&palette);
        for (row, ink) in glyph.iter().enumerate() {
            assert_eq!(
                palette.resolve(*ink),
                pixels.stops()[row],
                "row {row} disagrees between the surfaces"
            );
        }
    }

    #[test]
    fn a_theme_that_does_not_darken_keeps_its_backdrop_exactly() {
        let palette = Palette::default();
        let theme = themed("#40a060", None);
        assert_eq!(
            theme.grid_ramp(&palette).at(Length::NONE, Length(1.0)),
            Colour::rgb(0x40, 0xa0, 0x60)
        );
    }

    #[test]
    fn every_built_in_theme_parses() {
        for name in Theme::built_in_names() {
            let theme = Theme::built_in(name)
                .unwrap_or_else(|| panic!("{name} is not built in"))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(theme.name, name, "the file's name field disagrees");
            assert_eq!(theme.bars.len(), STOPS);
            assert_eq!(theme.grid.len(), STOPS);
            assert_eq!(theme.scope.len(), SCOPE_LEVELS);
        }
    }

    #[test]
    fn the_winamp_ramp_runs_green_to_red() {
        // Asserted on the file rather than on `Theme::default()`, which has been
        // through `darken`.
        let theme = Theme::parse(&raw("winamp")).unwrap();
        assert_eq!(theme.bars[0], Ink::Rgb(24, 132, 8), "foot is dark green");
        assert_eq!(theme.bars[STOPS - 1], Ink::Rgb(239, 49, 16), "tip is red");
        assert_eq!(theme.peak, Ink::Rgb(150, 150, 150), "caps are grey");
    }

    /// A built-in theme's text with its `darken` line removed, for comparing
    /// against the darkened result.
    fn raw(name: &str) -> String {
        BUILT_IN
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, text)| {
                text.lines()
                    .filter(|l| !l.trim_start().starts_with("darken"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .expect("built in")
    }

    #[test]
    fn the_theme_following_themes_name_no_absolute_colours() {
        // The whole point of them: the terminal decides what the colours are. An
        // Rgb value anywhere would ignore the user's theme.
        for name in ["terminal", "mono"] {
            let theme = Theme::built_in(name).unwrap().unwrap();
            let all = theme
                .bars
                .iter()
                .chain(&theme.grid)
                .chain(&theme.scope)
                .chain(std::iter::once(&theme.peak));
            for color in all {
                assert!(!matches!(color, Ink::Rgb(..)), "{name} used {color:?}");
            }
        }
    }

    #[test]
    fn a_backdrop_is_never_the_colour_of_the_bar_in_front_of_it() {
        // A grid stop equal to its bar stop makes the unlit column invisible, and
        // `g` a key that appears to do nothing.
        //
        // Checked against what is *drawn*, not what is parsed: `darken` is
        // applied at render time, so a theme may legitimately name its backdrop as
        // a copy of its bars and rely on the scaling to separate them.
        for name in Theme::built_in_names() {
            let theme = Theme::built_in(name).unwrap().unwrap();
            let drawn = crate::ui::analyzer::grid_colors(
                STOPS as u16,
                &theme,
                &crate::visual::Palette::default(),
            );
            for (i, (bar, grid)) in theme.bars.iter().zip(&drawn).enumerate() {
                assert_ne!(bar, grid, "{name}: stop {i} is invisible");
            }
        }
    }

    #[test]
    fn only_a_theme_that_needs_the_palette_asks_for_it() {
        // Asking writes escape sequences to the terminal and reads the replies
        // back, so the common case - a theme that spells its colours out - must
        // not do it at all.
        assert!(
            !Theme::default().needs_terminal_palette(),
            "the default theme is hex throughout"
        );
        assert!(
            !Theme::built_in("winamp")
                .unwrap()
                .unwrap()
                .needs_terminal_palette(),
            "winamp is hex and does not darken"
        );
        assert!(
            Theme::built_in("terminal")
                .unwrap()
                .unwrap()
                .needs_terminal_palette(),
            "terminal names its colours and darkens them"
        );
        assert!(
            !Theme::built_in("mono")
                .unwrap()
                .unwrap()
                .needs_terminal_palette(),
            "mono names colours but does not darken them"
        );
    }

    #[test]
    fn darken_is_carried_as_data_not_applied_here() {
        // Applying it needs the terminal's palette - a theme that named `green`
        // has no number to scale until something asks the terminal what its green
        // is. The parser records the intent and `grid_colors` carries it out.
        let plain = Theme::parse(&raw("terminal")).unwrap();
        assert_eq!(plain.darken, None);

        let asked = Theme::built_in("terminal").unwrap().unwrap();
        assert_eq!(asked.darken, Some(0.25), "terminal.toml asks for it");
        assert_eq!(asked.grid, plain.grid, "and the values are untouched here");
    }

    #[test]
    fn one_colour_stands_in_for_a_whole_ramp() {
        let theme = Theme::parse(
            r#"
            name = "flat"
            [colors]
            bars = "green"
            grid = "black"
            peak = "white"
            scope = "white"
            "#,
        )
        .expect("should parse");
        assert_eq!(theme.bars, vec![Ink::from_name("green").unwrap(); STOPS]);
        assert_eq!(
            theme.scope,
            vec![Ink::from_name("white").unwrap(); SCOPE_LEVELS]
        );
    }

    #[test]
    fn a_ramp_of_the_wrong_length_is_an_error() {
        // Padding it would put an arbitrary colour at the top of the ramp, which
        // reads as a rendering bug rather than as a broken file.
        let err = Theme::parse(
            r#"
            name = "short"
            [colors]
            bars = ["green", "red"]
            grid = "black"
            peak = "white"
            scope = "white"
            "#,
        )
        .expect_err("must not parse");
        let msg = format!("{err:#}");
        assert!(msg.contains("bars"), "unhelpful error: {msg}");
    }

    #[test]
    fn colours_are_hex_or_an_ansi_name() {
        assert_eq!(parse_color("#ef3110").unwrap(), Ink::Rgb(239, 49, 16));
        assert_eq!(
            parse_color("bright-green").unwrap(),
            Ink::from_name("bright-green").unwrap()
        );
        assert_eq!(
            parse_color(" white ").unwrap(),
            Ink::from_name("white").unwrap()
        );
        for bad in ["#fff", "#gggggg", "chartreuse", ""] {
            assert!(parse_color(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn an_unknown_theme_says_what_it_looked_for() {
        let err = Theme::load("no-such-theme").expect_err("must not resolve");
        assert!(format!("{err}").contains("no-such-theme"));
    }

    #[test]
    fn a_theme_loads_from_a_path() {
        let from_disk = Theme::load("themes/winamp.toml").expect("by path");
        let built_in = Theme::built_in("winamp").unwrap().unwrap();
        assert_eq!(from_disk, built_in, "the compiled-in copy has drifted");
    }
}
