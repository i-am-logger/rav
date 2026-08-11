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
}
