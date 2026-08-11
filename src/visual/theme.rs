//! Reading a theme from disk.
//!
//! The themes rav *ships* are consts, generated from `themes/*.toml` at build
//! time and living in [`rav_appearance`] - a target with no filesystem and no
//! TOML parser still has every one of them.
//!
//! This is the other half: reading a theme a user wrote. That needs a parser and
//! a filesystem, so it lives in the binary and not in a crate meant for a
//! microcontroller.
//!
//! `the_generated_themes_match_the_parser` holds the two together. A build
//! script that read `bright-cyan` differently from this parser would give a
//! binary whose built-in themes differ from the same file loaded by path, and
//! nothing else would notice.

use anyhow::{Context, Result, bail};
use rav_appearance::theme::{SCOPE_LEVELS, STOPS};
use rav_appearance::{Ink, Theme};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Load a theme: a built-in name, or a `.toml` file on disk.
///
/// Built-ins win over a file of the same name, so `--theme mono` cannot be
/// silently shadowed by a `mono.toml` that happens to be lying around.
pub fn load(spec: &str) -> Result<Theme> {
    if let Some(built_in) = Theme::built_in(spec) {
        return Ok(built_in);
    }
    let path = resolve(spec)
        .with_context(|| format!("no theme '{spec}': not built in, and no such file"))?;
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Turn a theme spec into the file it names, if one exists.
///
/// A bare name is looked up in `themes/` under the current directory - which is
/// the repo checkout during development - and then in `<config dir>/rav/themes/`,
/// which is where an installed rav finds one.
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

/// Parse a theme file.
pub fn parse(text: &str) -> Result<Theme> {
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

    Ok(Theme {
        name: file.name,
        bars,
        grid,
        peak,
        scope,
        darken: file.darken.floor(),
    })
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

    #[test]
    fn the_generated_themes_match_the_parser() {
        // Two readers of the same files: a build script that emits consts, and
        // this parser for a theme a user wrote. If they ever disagree about what
        // `bright-cyan` or `#188408` means, a built-in would differ from the
        // same file loaded by path - and nothing else in the tree would notice.
        for name in Theme::built_in_names() {
            let generated = Theme::built_in(name).expect("bundled");
            let text = std::fs::read_to_string(format!("crates/rav-appearance/themes/{name}.toml"))
                .expect("the bundled file is beside the crate");
            let parsed = parse(&text).expect("a bundled theme must parse");
            assert_eq!(
                generated, parsed,
                "{name} differs between generator and parser"
            );
        }
    }

    #[test]
    fn a_theme_can_be_loaded_by_path() {
        let from_disk = load("crates/rav-appearance/themes/winamp.toml").expect("by path");
        assert_eq!(from_disk, Theme::built_in("winamp").expect("bundled"));
    }

    #[test]
    fn a_built_in_name_wins_over_a_file_of_the_same_name() {
        assert_eq!(load("mono").expect("built in").name, "mono");
    }

    #[test]
    fn an_unknown_theme_says_so_rather_than_falling_back() {
        assert!(load("puce").is_err());
    }

    /// A theme file with `body` inside `[colors]`, otherwise well formed.
    fn theme_with(colors: &str) -> String {
        format!("name = \"test\"\n[colors]\npeak = \"#ffffff\"\n{colors}\n")
    }

    /// A TOML array of `count` identical colours.
    fn ladder(count: usize) -> String {
        let stops: Vec<&str> = (0..count).map(|_| "\"#ff0000\"").collect();
        format!("[{}]", stops.join(", "))
    }

    #[test]
    fn a_ladder_of_the_wrong_length_is_refused_rather_than_padded() {
        // The one error worth being strict about. Padding a short ramp puts a
        // colour the author never chose at the top - where a full-scale bar
        // reaches - and it reads as a rendering bug rather than as a file with
        // fifteen colours in it. The message has to say the number, because
        // miscounting the entries is how the mistake gets made.
        assert_eq!(ladder(15).matches("#ff0000").count(), 15, "fifteen stops");

        let file = |bars: String, scope: String| {
            theme_with(&format!(
                "bars = {bars}\ngrid = \"#000000\"\nscope = {scope}"
            ))
        };

        // Whichever ramp is not under test is given a length that is fine, so
        // the refusal can only be about the one being asked about.
        for wrong in [0, 1, 15, 17, 32] {
            let error = parse(&file(ladder(wrong), ladder(SCOPE_LEVELS)))
                .expect_err(&format!("{wrong} stops is not {STOPS}"));
            let said = format!("{error:#}");
            assert!(said.contains("bars"), "which ramp? {said}");
            assert!(said.contains(&STOPS.to_string()), "how many? {said}");
        }
        for wrong in [0, 1, 4, 6] {
            let error = parse(&file(ladder(STOPS), ladder(wrong)))
                .expect_err(&format!("{wrong} levels is not {SCOPE_LEVELS}"));
            assert!(format!("{error:#}").contains("scope"), "which ramp?");
        }

        // An array of one is not the single-colour form - that is a bare string.
        // Getting those two confused is the likeliest way to write this wrong,
        // so both are pinned: the array is refused and the string is not.
        assert!(parse(&file("[\"#ff0000\"]".into(), ladder(SCOPE_LEVELS))).is_err());
        assert!(parse(&file("\"#ff0000\"".into(), ladder(SCOPE_LEVELS))).is_ok());
    }

    #[test]
    fn one_colour_stands_in_for_a_whole_ladder() {
        // How `mono` is written, and the shortest possible theme file.
        let flat = parse(&theme_with(
            "bars = \"#123456\"\ngrid = \"black\"\nscope = \"#ffffff\"",
        ))
        .expect("a single colour is a ladder");
        assert_eq!(flat.bars.len(), STOPS);
        assert_eq!(flat.scope.len(), SCOPE_LEVELS);
        assert!(
            flat.bars
                .iter()
                .all(|&ink| ink == Ink::Rgb(0x12, 0x34, 0x56))
        );
    }

    #[test]
    fn a_colour_that_is_not_one_is_refused_with_the_two_ways_to_write_one() {
        // Every one of these is a plausible thing to type, and the error is the
        // only place a theme author finds out which forms exist.
        for bad in [
            "#fff",
            "#ffff",
            "#gggggg",
            "#ffffffff",
            "puce",
            "rgb(1,2,3)",
        ] {
            let error = parse(&theme_with(&format!(
                "bars = \"{bad}\"\ngrid = \"black\"\nscope = \"white\""
            )))
            .expect_err("{bad} is not a colour");
            let said = format!("{error:#}");
            assert!(
                said.contains("#rrggbb"),
                "{bad:?} was refused without saying how to write one: {said}",
            );
        }
    }

    #[test]
    fn the_named_colours_stay_named_all_the_way_through() {
        // The whole reason `terminal` and `mono` exist: a name is handed to the
        // surface unresolved, so the terminal paints it from the palette the
        // user actually runs. Resolving it here would replace their green with
        // one rav chose.
        let named = parse(&theme_with(
            "bars = \"bright-green\"\ngrid = \"black\"\nscope = \"white\"",
        ))
        .expect("names are colours");
        assert!(
            matches!(named.bars[0], Ink::Ansi(_)),
            "the name was resolved to an exact colour",
        );
    }

    #[test]
    fn darken_reads_all_three_ways_it_can_be_written() {
        // Absent, `true`, and a number are three different intents and one of
        // them is "leave it alone" - which is not the same as `Some(1.0)`.
        // Keeping them apart is what lets `mono` follow the terminal's own
        // palette: a theme that says nothing has its ink handed on untouched,
        // where a theme asking to keep all its brightness has it resolved and
        // scaled by one.
        let with = |line: &str| {
            parse(&format!(
                "name = \"t\"\n{line}\n[colors]\npeak = \"#ffffff\"\n\
                 bars = \"#ffffff\"\ngrid = \"#000000\"\nscope = \"#ffffff\"\n"
            ))
            .expect("valid")
            .darken
        };
        assert_eq!(with(""), None, "absent means leave it alone");
        assert_eq!(with("darken = true"), Some(DEFAULT_DARKEN));
        assert_eq!(with("darken = false"), None, "off is not zero");
        assert_eq!(with("darken = 0.4"), Some(0.4));
        assert_eq!(with("darken = 2.0"), Some(1.0), "clamped, not refused");
        assert_eq!(with("darken = -1.0"), Some(0.0));
    }

    #[test]
    fn a_theme_from_a_newer_rav_still_loads_on_an_older_one() {
        // Deliberate: the on-disk shape has no `deny_unknown_fields`, so a file
        // carrying a `[shapes]` table parses here and is ignored. That is what
        // lets skins ship without every older binary refusing every new theme
        // file - and it is a guarantee, so it is tested rather than assumed.
        let forward = parse(
            "name = \"future\"\ndescription = \"from later\"\n\
             [about]\nsource = \"someone else\"\n\
             [shapes]\nsteps = 8\nglyphs = \"blocks\"\n\
             [colors]\npeak = \"#ffffff\"\nbars = \"#ff0000\"\n\
             grid = \"#000000\"\nscope = \"#ffffff\"\nsparkle = \"#ff00ff\"\n",
        )
        .expect("unknown keys are for whoever reads the file next");
        assert_eq!(forward.name, "future");
    }

    #[test]
    fn the_theme_file_in_the_documentation_is_one_that_works() {
        // docs/themes.md hands a whole `sunset.toml` to anyone writing their
        // first theme, so a document that disagrees with the parser makes their
        // first attempt fail with rav's mistake in their file. Extracted from
        // the document rather than copied here, so this checks the thing people
        // read rather than a copy of it.
        let doc = std::fs::read_to_string("docs/themes.md").expect("beside the source");
        let example = doc
            .split("```toml")
            .map(|block| block.split("```").next().unwrap_or_default())
            .find(|block| block.contains("name = \"sunset\""))
            .expect("the worked example is still in docs/themes.md");

        let theme = parse(example).expect("the documented example must parse");
        assert_eq!(theme.name, "sunset");
        assert_eq!(theme.bars.len(), STOPS);
        assert_eq!(theme.grid.len(), STOPS, "one colour fills the ladder");
        assert_eq!(theme.scope.len(), SCOPE_LEVELS);
    }
}
