/// The ANSI names a theme may use, in slot order.
///
/// The names rather than a terminal library's own spelling: `white` is slot 7
/// and `bright-white` is slot 15, which is what a theme author sees in their
/// configuration. A library that calls those `Gray` and `White` is describing
/// its own type, not the terminal.
///
/// In a file of its own because `build.rs` reads it too, with `include!`. The
/// build script turns the bundled themes into consts so a target with no TOML
/// parser can still have them, and it must agree with the runtime parser about
/// what `bright-cyan` means. One table, included twice, cannot disagree with
/// itself - and `the_generated_themes_match_the_parser` checks the rest.
pub const ANSI_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-magenta",
    "bright-cyan",
    "bright-white",
];
