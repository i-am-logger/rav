//! Skins: the colours the display is drawn in, as data.
//!
//! A skin is a TOML file. Everything rav ships is one of these — nothing about
//! the palette lives in code — so contributing a skin means writing a file, not
//! touching Rust. `docs/skins.md` is the format's documentation; this module is
//! its implementation.
//!
//! Colours are either `"#rrggbb"`, which looks the same everywhere, or one of
//! the sixteen ANSI names, which the terminal paints from the active theme. A
//! skin can mix them freely: that choice is the difference between reproducing a
//! specific look and following whatever the user already runs.

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Stops in the analyser ramp, and in a per-stop backdrop.
pub const STOPS: usize = 16;
/// Levels in the oscilloscope ladder.
pub const SCOPE_LEVELS: usize = 5;

/// The skins rav ships, compiled in so the binary needs nothing on disk. First
/// is the default, and the order is the one `s` cycles.
///
/// `rav.toml` and `winamp.toml` carry their own attribution blocks - that is
/// where the MIT notice for the transcribed ramp lives, there being no vendored
/// file to put it alongside.
const BUILT_IN: [(&str, &str); 4] = [
    ("rav", include_str!("../../skins/rav.toml")),
    ("winamp", include_str!("../../skins/winamp.toml")),
    ("terminal", include_str!("../../skins/terminal.toml")),
    ("mono", include_str!("../../skins/mono.toml")),
];

/// A parsed skin: every colour the display needs, resolved and length-checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub name: String,
    /// Analyser ramp, foot of the bar first.
    pub bars: Vec<Color>,
    /// Backdrop per stop, aligned with `bars`.
    pub grid: Vec<Color>,
    pub peak: Color,
    /// Oscilloscope ladder, centre of the trace first.
    pub scope: Vec<Color>,
    /// How much brightness the backdrop keeps, when the skin asked to darken it.
    ///
    /// Applied at render time rather than here, because darkening a colour the
    /// skin only *named* needs the terminal's actual palette - see
    /// [`crate::visual::Theme`].
    pub darken: Option<f32>,
}

impl Default for Skin {
    fn default() -> Self {
        // Validated by `every_built_in_skin_parses`, so a failure here means the
        // build embedded something unexpected.
        Self::built_in("rav")
            .expect("rav is built in")
            .expect("the bundled rav skin must parse")
    }
}

impl Skin {
    /// Names of the compiled-in skins, in the order `s` cycles them.
    pub fn built_in_names() -> impl Iterator<Item = &'static str> {
        BUILT_IN.iter().map(|(name, _)| *name)
    }

    /// One of the compiled-in skins by name.
    pub fn built_in(name: &str) -> Option<Result<Self>> {
        BUILT_IN
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(n, text)| Self::parse(text).with_context(|| format!("built-in skin '{n}'")))
    }

    /// Load a skin: a built-in name, or a `.toml` file on disk.
    ///
    /// Built-ins win over a file of the same name, so `--skin mono` cannot be
    /// silently shadowed by a `skins/mono.toml` that happens to be lying around.
    pub fn load(spec: &str) -> Result<Self> {
        if let Some(built_in) = Self::built_in(spec) {
            return built_in;
        }
        let path = Self::resolve(spec)
            .with_context(|| format!("no skin '{spec}': not built in, and no such file"))?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Turn a skin spec into the file it names, if one exists.
    ///
    /// A bare name is looked up in `skins/` under the current directory - which
    /// is the repo checkout during development - and then in
    /// `<config dir>/rav/skins/`, which is where an installed rav finds one.
    fn resolve(spec: &str) -> Option<PathBuf> {
        let direct = Path::new(spec);
        if direct.is_file() {
            return Some(direct.to_path_buf());
        }
        let mut roots = vec![PathBuf::from("skins")];
        if let Some(dir) = dirs::config_dir() {
            roots.push(dir.join("rav").join("skins"));
        }
        roots.into_iter().find_map(|root| {
            let named = root.join(format!("{spec}.toml"));
            named.is_file().then_some(named)
        })
    }

    /// Whether rendering this skin needs the terminal's palette read.
    ///
    /// Only a skin that both asks to darken *and* names a colour rather than
    /// spelling it out: there is nothing to look up otherwise, and asking is not
    /// free - it writes escape sequences and reads the replies off the terminal.
    pub fn needs_terminal_palette(&self) -> bool {
        self.darken.is_some() && self.grid.iter().any(|c| !matches!(c, Color::Rgb(..)))
    }

    /// Parse a skin file.
    pub fn parse(text: &str) -> Result<Self> {
        let file: File = toml::from_str(text).context("not a valid skin file")?;

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

        let skin = Self {
            name: file.name,
            bars,
            grid,
            peak,
            scope,
            darken: file.darken.floor(),
        };
        Ok(skin)
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

/// Deepen the shadow end of a skin's palette.
///
/// `true` for the default strength, or a number in `0.0..=1.0` for how far the
/// darkest colour is pushed towards black - `0.25` means it keeps a quarter of
/// its brightness. Skins written for a 16-pixel panel on a CRT tend to read too
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
    /// The floor factor, or `None` when the skin is left alone.
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
    bars: Ramp,
    grid: Ramp,
    peak: String,
    scope: Ramp,
}

/// One colour, or a full ladder of them.
#[derive(Deserialize)]
#[serde(untagged)]
enum Ramp {
    One(String),
    Many(Vec<String>),
}

impl Default for Ramp {
    fn default() -> Self {
        Ramp::Many(Vec::new())
    }
}

impl Ramp {
    /// Resolve to exactly `len` colours.
    ///
    /// A single colour repeats; a ladder must already be the right length. Silently
    /// padding a short one would put an arbitrary colour at the top of the ramp,
    /// which is exactly the sort of thing that looks like a rendering bug later.
    fn expand(&self, len: usize) -> Result<Vec<Color>> {
        match self {
            Ramp::One(name) => Ok(vec![parse_color(name)?; len]),
            Ramp::Many(names) => {
                if names.len() != len {
                    bail!("needs 1 or {len} colours, found {}", names.len());
                }
                names.iter().map(|n| parse_color(n)).collect()
            }
        }
    }
}

/// `#rrggbb`, or one of the sixteen ANSI names.
fn parse_color(text: &str) -> Result<Color> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() != 6 {
            bail!("{text:?}: a hex colour is #rrggbb");
        }
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16);
        let (r, g, b) = (channel(0), channel(2), channel(4));
        return match (r, g, b) {
            (Ok(r), Ok(g), Ok(b)) => Ok(Color::Rgb(r, g, b)),
            _ => bail!("{text:?}: a hex colour is #rrggbb"),
        };
    }
    // `white`/`bright-black` rather than ratatui's `Gray`/`DarkGray`: the ANSI
    // names are what a skin author is reading off their theme.
    Ok(match text {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::Gray,
        "bright-black" => Color::DarkGray,
        "bright-red" => Color::LightRed,
        "bright-green" => Color::LightGreen,
        "bright-yellow" => Color::LightYellow,
        "bright-blue" => Color::LightBlue,
        "bright-magenta" => Color::LightMagenta,
        "bright-cyan" => Color::LightCyan,
        "bright-white" => Color::White,
        other => bail!("{other:?} is not a colour: use #rrggbb or an ANSI name"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_skin_parses() {
        for name in Skin::built_in_names() {
            let skin = Skin::built_in(name)
                .unwrap_or_else(|| panic!("{name} is not built in"))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(skin.name, name, "the file's name field disagrees");
            assert_eq!(skin.bars.len(), STOPS);
            assert_eq!(skin.grid.len(), STOPS);
            assert_eq!(skin.scope.len(), SCOPE_LEVELS);
        }
    }

    #[test]
    fn the_winamp_ramp_runs_green_to_red() {
        // Asserted on the file rather than on `Skin::default()`, which has been
        // through `darken`.
        let skin = Skin::parse(&raw("winamp")).unwrap();
        assert_eq!(skin.bars[0], Color::Rgb(24, 132, 8), "foot is dark green");
        assert_eq!(skin.bars[STOPS - 1], Color::Rgb(239, 49, 16), "tip is red");
        assert_eq!(skin.peak, Color::Rgb(150, 150, 150), "caps are grey");
    }

    /// A built-in skin's text with its `darken` line removed, for comparing
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
    fn the_theme_following_skins_name_no_absolute_colours() {
        // The whole point of them: the terminal decides what the colours are. An
        // Rgb value anywhere would ignore the user's theme.
        for name in ["terminal", "mono"] {
            let skin = Skin::built_in(name).unwrap().unwrap();
            let all = skin
                .bars
                .iter()
                .chain(&skin.grid)
                .chain(&skin.scope)
                .chain(std::iter::once(&skin.peak));
            for color in all {
                assert!(!matches!(color, Color::Rgb(..)), "{name} used {color:?}");
            }
        }
    }

    #[test]
    fn a_backdrop_is_never_the_colour_of_the_bar_in_front_of_it() {
        // A grid stop equal to its bar stop makes the unlit column invisible, and
        // `g` a key that appears to do nothing.
        //
        // Checked against what is *drawn*, not what is parsed: `darken` is
        // applied at render time, so a skin may legitimately name its backdrop as
        // a copy of its bars and rely on the scaling to separate them.
        for name in Skin::built_in_names() {
            let skin = Skin::built_in(name).unwrap().unwrap();
            let drawn = crate::ui::analyzer::grid_colors(
                STOPS as u16,
                &skin,
                &crate::visual::Theme::default(),
            );
            for (i, (bar, grid)) in skin.bars.iter().zip(&drawn).enumerate() {
                assert_ne!(bar, grid, "{name}: stop {i} is invisible");
            }
        }
    }

    #[test]
    fn only_a_skin_that_needs_the_palette_asks_for_it() {
        // Asking writes escape sequences to the terminal and reads the replies
        // back, so the common case - a skin that spells its colours out - must
        // not do it at all.
        assert!(
            !Skin::default().needs_terminal_palette(),
            "the default skin is hex throughout"
        );
        assert!(
            !Skin::built_in("winamp")
                .unwrap()
                .unwrap()
                .needs_terminal_palette(),
            "winamp is hex and does not darken"
        );
        assert!(
            Skin::built_in("terminal")
                .unwrap()
                .unwrap()
                .needs_terminal_palette(),
            "terminal names its colours and darkens them"
        );
        assert!(
            !Skin::built_in("mono")
                .unwrap()
                .unwrap()
                .needs_terminal_palette(),
            "mono names colours but does not darken them"
        );
    }

    #[test]
    fn darken_is_carried_as_data_not_applied_here() {
        // Applying it needs the terminal's palette - a skin that named `green`
        // has no number to scale until something asks the terminal what its green
        // is. The parser records the intent and `grid_colors` carries it out.
        let plain = Skin::parse(&raw("terminal")).unwrap();
        assert_eq!(plain.darken, None);

        let asked = Skin::built_in("terminal").unwrap().unwrap();
        assert_eq!(asked.darken, Some(0.25), "terminal.toml asks for it");
        assert_eq!(asked.grid, plain.grid, "and the values are untouched here");
    }

    #[test]
    fn one_colour_stands_in_for_a_whole_ramp() {
        let skin = Skin::parse(
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
        assert_eq!(skin.bars, vec![Color::Green; STOPS]);
        assert_eq!(skin.scope, vec![Color::Gray; SCOPE_LEVELS]);
    }

    #[test]
    fn a_ramp_of_the_wrong_length_is_an_error() {
        // Padding it would put an arbitrary colour at the top of the ramp, which
        // reads as a rendering bug rather than as a broken file.
        let err = Skin::parse(
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
        assert_eq!(parse_color("#ef3110").unwrap(), Color::Rgb(239, 49, 16));
        assert_eq!(parse_color("bright-green").unwrap(), Color::LightGreen);
        assert_eq!(parse_color(" white ").unwrap(), Color::Gray);
        for bad in ["#fff", "#gggggg", "chartreuse", ""] {
            assert!(parse_color(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn an_unknown_skin_says_what_it_looked_for() {
        let err = Skin::load("no-such-skin").expect_err("must not resolve");
        assert!(format!("{err}").contains("no-such-skin"));
    }

    #[test]
    fn a_skin_loads_from_a_path() {
        let from_disk = Skin::load("skins/winamp.toml").expect("by path");
        let built_in = Skin::built_in("winamp").unwrap().unwrap();
        assert_eq!(from_disk, built_in, "the compiled-in copy has drifted");
    }
}
