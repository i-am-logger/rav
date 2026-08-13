# Developing rav

## Build and run

```
cargo build --release && ./target/release/rav
```

On Linux that needs `alsa-lib` and `pkg-config` on the system — cpal links the
ALSA backend. The flake and the dev shell provide both; a bare `cargo build` or
`cargo install rav` does not.

Or through the flake, which pins the toolchain and the Linux ALSA headers:

```
nix build .#default && ./result/bin/rav
nix run github:i-am-logger/rav      # straight from the repo, nothing checked out
```

## The dev shell

`direnv allow` loads it automatically; `devenv shell` does the same by hand. It
brings the Rust toolchain, `ffmpeg`, `sox` and `cargo-watch`, and adds:

| Script | |
|---|---|
| `visual-watch` | rebuild and relaunch rav on every save |
| `check-audio` | report what rav can capture from |
| `install-background-music` | set up macOS system-audio capture |
| `test-audio-pipeline` | exercise capture and processing end to end |
| `record-demo` | re-record `assets/demo.gif` |
| `dev-test` / `dev-run` / `dev-profile` | the obvious three |

`check-audio` runs on shell entry, so an audio setup problem shows up before you
build rather than after.

## Checks

```
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
nix build .#default
```

**`--workspace` is not optional.** Without it both commands take only the root
package and the two member crates go untested and unlinted — silently, and with a
green result. 54 tests stopped running that way once.

`clippy` and `rustfmt` also run as git hooks, and CI runs `devenv test` on Linux —
the same command you can run locally, which is where the
`#[cfg(target_os = "macos")]` mistakes surface.

CI does **not** build the flake, so run `nix build .#default` yourself before a
push that touches packaging. Flakes only see **git-tracked** files, so a new asset
that has not been `git add`ed fails there and nowhere else — and `nix run
github:i-am-logger/rav` is the install path in the README.

## Recording the demo

`record-demo` screen-records the frontmost window and quantises it to a GIF, and
it drives rav while filming rather than capturing one static view.
`RAV_DEMO_TOUR` is a list of `seconds:key` steps — hold that long, then send that
key, with `-` for "send nothing". The default walks the four themes and the
four viewing angles, then the oscilloscope and the help overlay, and returns to
the defaults so the loop is seamless.

Size the window to 941×249 first; that is the shape the committed GIF was shot
at, so a re-record drops straight in.

| Variable | |
|---|---|
| `RAV_DEMO_TOUR` | the `seconds:key` script; its holds set the duration |
| `RAV_DEMO_GEOMETRY` | `WxH+X+Y` in points; defaults to the frontmost window |
| `RAV_DEMO_SCALE` | `2` on a Retina display — capture is in pixels, geometry in points |
| `RAV_DEMO_FPS` | 25 by default, and it sets the capture rate too; see below |
| `RAV_DEMO_OUT`, `RAV_DEMO_LEAD`, `RAV_DEMO_SETTLE` | the rest |

**Record it in a terminal that can draw images** — WezTerm, Ghostty or kitty.
rav draws pixels wherever the terminal says it can, so a recording made anywhere
else shows the fallback rather than the thing: block characters, a peak cap
erasing the grid it crosses, and four seconds of `v` doing nothing but printing
`needs pixels`. The committed `assets/demo.gif` predates the pixel surface
entirely and was shot in Terminal.app, which cannot even show rav's colours —
it reports `colors#256` and no `Tc`.

The capture and the GIF run at the same rate — one variable, so they cannot
drift. Capturing above the GIF rate and sampling back down throws away real
motion; capturing below it cannot be recovered downstream.

25 fps is the default because that is what a terminal actually produces.
Measured by per-frame checksums over five seconds of the same rav build:

| | distinct frames | |
|---|---|---|
| Terminal.app | 113 of 300 | ~23 fps |
| WezTerm | 258 of 300 | ~52 fps |

The render loop is scheduled at `display.refresh_rate` (60), so the terminal's
redraw rate is the ceiling, not rav. Record in a GPU-accelerated terminal and
raise `RAV_DEMO_FPS` to 50 if the GIF needs to be smoother.

Do not go above 50. GIF stores frame delay in whole centiseconds: 25 fps is
exactly 4cs and 50 is exactly 2cs, but 60 would be 1.67cs and is written as an
uneven 2/1 cadence that reads as judder. Check the delays in the finished file
rather than assuming the number survived — the committed GIF is 650 frames at a
uniform 4cs, 633 of which differ from the one before.

macOS needs Screen Recording permission for the capture and Accessibility for
the window geometry and the keystrokes; both are per-terminal. Linux uses
`x11grab` and wants `xdotool` on `PATH`, or `RAV_DEMO_GEOMETRY` set by hand.

## Layout

Three crates. The split is not organisation for its own sake: `rav-core` and
`rav-appearance` are `no_std`, so neither can acquire a terminal, a clock or an
allocator by accident — which a module and a doc comment demonstrably could not
prevent.

| Path | |
|---|---|
| `crates/rav-core/` | **`no_std`, no allocator.** The mechanics: `Level`, `Step`, `Fill`, `Length`, geometry, and what a display can show. Knows *how many* and *which one*, never what any of them look like |
| `crates/rav-appearance/` | **`no_std`.** What a step looks like: inks, colours, ramps, the scene, and themes. Names no rasteriser, so one theme dresses a glyph grid, a window and an LED strip alike |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, turned into consts by that crate's `build.rs` — see [themes.md](themes.md) |
| `assets/skins/` | the built-in skin artwork, embedded with `include_str!` — see [skins.md](skins.md) |
| `src/audio/` | cpal capture, and the macOS CoreAudio process tap — see [audio.md](audio.md) |
| `src/signal/` | FFT front end, bin-to-bar mapping, bar and cap ballistics |
| `src/visual/` | reading a theme a *user* wrote, and asking the terminal for its palette — the halves that need a filesystem and a terminal |
| `src/ui/` | the `App` event loop and the analyser, scope, status and help widgets |
| `src/render/` | the rasteriser: a `Canvas` over tiny-skia, and the one impl that puts a scene onto it. Knows nothing about terminals |
| `src/surface/` | where a frame goes. Asking the terminal what it can draw, composing a frame of pixels, and the kitty protocol that carries one — see the module note in `surface/pixels.rs` for the rules the terminal imposes |
| `src/testing/` | signal sources with known ground truth — tones, noise, and notes by MIDI number |
| `tests/`, `crates/rav-core/tests/` | the tests that have to see a crate from outside, and the ones that hold for *every* input rather than a chosen one |

Everything lives in the library — `src/main.rs` is argument parsing and wiring —
so the binary and the tests compile one copy of the tree rather than two that can
drift apart.

## Tests

Three kinds, and where a new one goes follows from what it needs to see:

| | |
|---|---|
| beside the code, `#[cfg(test)]` | most of them. Anything reaching a private field, and everything that documents what a type does |
| `tests/` | anything that has to see a crate the way a dependent does, and the properties that hold for *every* input. `proptest` is a dev-dependency of the workspace |
| `src/testing/` | the signal sources those drive: tones, noise, and notes by MIDI number, where the expected bar is derivable from the pitch rather than guessed |

`App` swaps its terminal for ratatui's `TestBackend` under `#[cfg(test)]`, so a
test that renders frames has to be inline — an integration test gets the real
backend and no terminal.

**Check a new test by breaking the code it covers, not by watching it pass.** A
test that cannot fail is worse than none, because it reads as coverage. Several
here were written, passed first time, and were measuring nothing: a peak test
sampled the midpoints of a stripe, which is the half of the range where the bug
it was written for cannot appear; a rounding sweep reported an overshoot of
exactly zero because the case it looked for needs both ends of an interpolation
to be the same bin. Both passed. Neither asserted anything.

So: make the change the test forbids, watch it fail, and put it back. Where that
has been done the reasoning is in the commit, and where a test rests on a float
comparison rather than an inequality with room in it, the measured margin is in
the test.

## Releasing

`release-plz` owns the version, `CHANGELOG.md`, the tag, the GitHub release and
the crates.io publish — one tool, reading the baseline from crates.io rather than
from a file in the repo.

Merging to master runs `release-plz release`, which publishes only if the version
in `Cargo.toml` is ahead of the registry. It then opens a Release PR proposing the
next version from the commits since the last tag; merging *that* PR is what
authorises the next publish. `semver_check` rejects a bump smaller than the API
change deserves.

The previous setup was release-please plus a separate publish step, and it failed
in a way worth remembering: the version lived in
`.release-please-manifest.json`, so when the publish step broke the repo kept
tagging releases that never reached the registry. rav has tags `rav-v0.2.0`
through `rav-v0.2.4` and has never existed on crates.io.

Those tags have been deleted, along with their GitHub releases. A crate that is
publishable, carries release tags and has no crates.io row is a state release-plz
cannot resolve: it reads the next-version baseline from the registry and finds
nothing where the tags say there should be something. With the orphans gone the
registry and the repository agree — both say rav has never been released — and
`1.0.0-beta.1` is the first.

The rule that follows: never tag a release that has not reached the registry. The
publish step and the tag are one operation now, which is the point of using
release-plz rather than a tool that tags from a file it keeps itself.
