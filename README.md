<div align="center">

# rav

**Rust audio visualiser** — a real-time spectrum analyser with a retro look.

Bars, peak caps and an oscilloscope, in truecolour.
Terminal today, GUI next.

![rav](https://raw.githubusercontent.com/i-am-logger/rav/master/assets/demo.gif)

[![crates.io](https://img.shields.io/crates/v/rav?logo=rust&logoColor=white)](https://crates.io/crates/rav)
[![Downloads](https://img.shields.io/crates/d/rav?logo=rust&logoColor=white)](https://crates.io/crates/rav)
[![MSRV](https://img.shields.io/crates/msrv/rav?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/i-am-logger/rav/ci.yml?branch=master&label=CI&logo=githubactions&logoColor=white)](https://github.com/i-am-logger/rav/actions/workflows/ci.yml)
[![docs.rs](https://img.shields.io/docsrs/rav?logo=docsdotrs&logoColor=white)](https://docs.rs/rav)

[![Nix](https://img.shields.io/badge/Nix-2b2b2b?logo=nixos&logoColor=white)](https://nixos.org)
[![devenv](https://img.shields.io/badge/devenv-2b2b2b?logo=nixos&logoColor=white)](https://devenv.sh)
[![Linux](https://img.shields.io/badge/Linux-2b2b2b?logo=linux&logoColor=white)](docs/audio.md)
[![macOS](https://img.shields.io/badge/macOS-2b2b2b?logo=apple&logoColor=white)](docs/audio.md)
[![License: CC BY-NC-SA 4.0](https://img.shields.io/badge/CC%20BY--NC--SA%204.0-2b2b2b?logo=creativecommons&logoColor=white)](LICENSE)

</div>

---

## Install

```
nix run github:i-am-logger/rav      # no install
cargo install rav                   # needs alsa-lib and pkg-config on Linux
```

Or from source: `cargo build --release && ./target/release/rav`.

## Usage

```
rav                   # analyser
rav --theme winamp    # rav, winamp, terminal, mono, or a path to a theme
rav --list-devices    # what rav can capture from
rav -d "<name>"       # capture from a named device
rav --clean           # log almost nothing
rav --surface glyphs  # auto, glyphs, kitty or window
rav --skin bar.svg    # draw the bars from your own shape
```

`--skin` takes an SVG and builds the bars from it. Only the shape is used, so the
theme still decides the colour, and it needs a terminal drawing pixels — see
**[docs/skins.md](docs/skins.md)**.

A terminal that can draw images gets **pixels**: a peak cap rides over the
backdrop instead of erasing the cell it crosses, the bar ladder is even at any
font size, and `v` bolts the field at an angle. WezTerm, Ghostty and kitty can;
everything else draws block characters, as does a terminal that will not report
its size in pixels. `--surface glyphs` forces the block characters. Press `h` for
which surface rav is on and why.

`--surface window` draws the same picture in a window of rav's own, in a build
that has one. It is off by default because a terminal visualiser should not make
everyone who installs it build a windowing stack.

```
cargo install rav --features gui && rav --surface window
cargo run --features gui -- --surface window   # from a clone
```

Everything else is a keypress, and `h` shows the lot with their current values:

| Key | |
|---|---|
| `q` / `Esc` | quit |
| `Space` / `Tab` / `o` | switch analyser ↔ oscilloscope |
| `t` | theme — rav, winamp, terminal, mono |
| `b` | bar style — blocks, solid, thick, half, line, shade |
| `v` | viewing angle — flat, raked, turned, corridor, swaying; pixels only |
| `p` | peak caps — fine, coarse, off |
| `g` | grid behind the bars, on/off |
| `w` | bandwidth: wide (grouped) or thin |
| `f` | bar fall speed — 3, 6, 12, 16, 32 |
| `r` | frequency range — 8/12/16/20 kHz or full |
| `↑` / `↓` | gain trim, `0` resets |
| `+` / `-` | bar size |
| `h` / `?` | help |

Logs go to `rav.log`, never to the display.

## Themes

Four, cycled with `t` and all compiled in — the binary needs nothing on disk:

| | |
|---|---|
| **rav** *(default)* | the classic ramp, over a backdrop of its own colours dimmed |
| **winamp** | the faithful reproduction of the original: one flat dim backdrop |
| **terminal** | the terminal's own 16 colours, so rav follows your theme |
| **mono** | greys only; height alone carries the level |

A theme is a TOML file, not code — `rav --theme ./sunset.toml`. See
**[docs/themes.md](docs/themes.md)** and send one.

## Capturing system audio

Nothing to install — see **[docs/audio.md](docs/audio.md)**. If it stays silent,
**[docs/troubleshooting.md](docs/troubleshooting.md)**.

## Building and hacking on it

See **[docs/developing.md](docs/developing.md)**.

## Attribution

The ballistics, and the ramp the `rav` and `winamp` themes use, come from
[Webamp](https://github.com/captbaritone/webamp) — MIT, © 2015 Jordan Eldredge.

## What's next

Small displays — a strip of LEDs, or a matrix — which the same themes already
cover, since what rav measures is kept apart from what it looks like.

## License

CC BY-NC-SA 4.0 — see [LICENSE](LICENSE).
