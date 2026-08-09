<div align="center">

# rav

**Rust audio visualiser** — a real-time spectrum analyser with a retro look.

Bars, peak caps and an oscilloscope, in truecolour.
Terminal today, GUI next.

![rav](assets/demo.gif)

[![CI](https://img.shields.io/github/actions/workflow/status/i-am-logger/rav/ci.yml?branch=master&label=CI&logo=githubactions&logoColor=white)](https://github.com/i-am-logger/rav/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-2b2b2b?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Nix](https://img.shields.io/badge/Nix-2b2b2b?logo=nixos&logoColor=white)](https://nixos.org)
[![License: CC BY-NC-SA 4.0](https://img.shields.io/badge/CC%20BY--NC--SA%204.0-2b2b2b?logo=creativecommons&logoColor=white)](LICENSE)

[![Linux](https://img.shields.io/badge/Linux-2b2b2b?logo=linux&logoColor=white)](docs/audio.md)
[![macOS](https://img.shields.io/badge/macOS-2b2b2b?logo=apple&logoColor=white)](docs/audio.md)

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
rav --skin winamp     # rav, winamp, terminal, mono, or a path to a skin
rav --list-devices    # what rav can capture from
rav -d "<name>"       # capture from a named device
rav --clean           # keep logging out of the display
```

Everything else is a keypress, and `h` shows the lot with their current values:

| Key | |
|---|---|
| `q` / `Esc` | quit |
| `Space` / `Tab` / `o` | switch analyser ↔ oscilloscope |
| `s` | skin — rav, winamp, terminal, mono |
| `b` | bar style — blocks, solid, thick, half, line, shade |
| `p` | peak caps — fine, coarse, off |
| `g` | grid behind the bars, on/off |
| `w` | bandwidth: wide (grouped) or thin |
| `f` | bar fall speed — 3, 6, 12, 16, 32 |
| `r` | frequency range — 8/12/16/20 kHz or full |
| `↑` / `↓` | gain trim, `0` resets |
| `h` / `?` | help |

Logs go to `rav.log`, never to the display.

## Skins

Four, cycled with `s` and all compiled in — the binary needs nothing on disk:

| | |
|---|---|
| **rav** *(default)* | the classic ramp, over a backdrop of its own colours dimmed |
| **winamp** | the faithful reproduction of the original: one flat dim backdrop |
| **terminal** | the terminal's own 16 colours, so rav follows your theme |
| **mono** | greys only; height alone carries the level |

A skin is a TOML file, not code — `rav --skin ./sunset.toml`. See
**[docs/skins.md](docs/skins.md)** and send one.

## Capturing system audio

Nothing to install — see **[docs/audio.md](docs/audio.md)**.

## How it works

```
capture → mono → Hann-windowed 1024-sample FFT → per-bin tilt
        → 91% log / 9% linear bar map → bar and cap ballistics
        → one ratatui widget, written straight into the cell buffer
```

Each stage documents its own constants, in `src/signal/`.

## Building and hacking on it

See **[docs/developing.md](docs/developing.md)**.

## Attribution

The ballistics, and the ramp the `rav` and `winamp` skins use, come from
[Webamp](https://github.com/captbaritone/webamp) — MIT, © 2015 Jordan Eldredge.

## What's next

A GUI. Only `src/ui/` knows about terminal cells, so the same display and the
same skins can be drawn anywhere.

## License

CC BY-NC-SA 4.0 — see [LICENSE](LICENSE).
