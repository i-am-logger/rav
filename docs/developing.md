| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |# Developing rav
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |## Build and run
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |```
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |cargo build --release && ./target/release/rav
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |```
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |On Linux that needs `alsa-lib` and `pkg-config` on the system — cpal links the
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |ALSA backend. The flake and the dev shell provide both; a bare `cargo build` or
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`cargo install rav` does not.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Or through the flake, which pins the toolchain and the Linux ALSA headers:
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |```
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |nix build .#default && ./result/bin/rav
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |nix run github:i-am-logger/rav      # straight from the repo, nothing checked out
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |```
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |## The dev shell
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`direnv allow` loads it automatically; `devenv shell` does the same by hand. It
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |brings the Rust toolchain, `ffmpeg`, `sox` and `cargo-watch`, and adds:
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || Script | |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) ||---|---|
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `visual-watch` | rebuild and relaunch rav on every save |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `check-audio` | report what rav can capture from |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `install-background-music` | set up macOS system-audio capture |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `test-audio-pipeline` | exercise capture and processing end to end |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `record-demo` | re-record `assets/demo.gif` |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `dev-test` / `dev-run` / `dev-profile` | the obvious three |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`check-audio` runs on shell entry, so an audio setup problem shows up before you
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |build rather than after.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |## Checks
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |```
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |cargo test
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |cargo clippy --all-targets --all-features -- -D warnings
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |cargo fmt --check
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |nix build .#default
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |```
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`clippy` and `rustfmt` also run as git hooks, and CI runs `cargo fmt --check`,
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`cargo clippy --all-targets -- -D warnings` and `cargo test` on Linux — which is
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |where the `#[cfg(target_os = "macos")]` mistakes surface.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |CI does **not** build the flake, so run `nix build .#default` yourself before a
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |push that touches packaging. Flakes only see **git-tracked** files, so a new asset
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |that has not been `git add`ed fails there and nowhere else — and `nix run
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |github:i-am-logger/rav` is the install path in the README.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |## Recording the demo
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`record-demo` screen-records the frontmost window and quantises it to a GIF, and
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |it drives rav while filming rather than capturing one static view.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`RAV_DEMO_TOUR` is a list of `seconds:key` steps — hold that long, then send that
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |key, with `-` for "send nothing". The default walks the three themes, the
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |oscilloscope and the help overlay, then returns to the defaults so the loop is
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |seamless.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Size the window to 941×249 first; that is the shape the committed GIF was shot
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |at, so a re-record drops straight in.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || Variable | |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) ||---|---|
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `RAV_DEMO_TOUR` | the `seconds:key` script; its holds set the duration |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `RAV_DEMO_GEOMETRY` | `WxH+X+Y` in points; defaults to the frontmost window |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `RAV_DEMO_SCALE` | `2` on a Retina display — capture is in pixels, geometry in points |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `RAV_DEMO_FPS` | 25 by default, and it sets the capture rate too; see below |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `RAV_DEMO_OUT`, `RAV_DEMO_LEAD`, `RAV_DEMO_SETTLE` | the rest |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |The capture and the GIF run at the same rate — one variable, so they cannot
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |drift. Capturing above the GIF rate and sampling back down throws away real
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |motion; capturing below it cannot be recovered downstream.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |25 fps is the default because that is what a terminal actually produces.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Measured by per-frame checksums over five seconds of the same rav build:
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || | distinct frames | |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) ||---|---|---|
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || Terminal.app | 113 of 300 | ~23 fps |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || WezTerm | 258 of 300 | ~52 fps |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |The render loop is scheduled at `display.refresh_rate` (60), so the terminal's
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |redraw rate is the ceiling, not rav. Record in a GPU-accelerated terminal and
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |raise `RAV_DEMO_FPS` to 50 if the GIF needs to be smoother.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Do not go above 50. GIF stores frame delay in whole centiseconds: 25 fps is
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |exactly 4cs and 50 is exactly 2cs, but 60 would be 1.67cs and is written as an
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |uneven 2/1 cadence that reads as judder. Check the delays in the finished file
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |rather than assuming the number survived — the committed GIF is 650 frames at a
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |uniform 4cs, 633 of which differ from the one before.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |macOS needs Screen Recording permission for the capture and Accessibility for
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |the window geometry and the keystrokes; both are per-terminal. Linux uses
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`x11grab` and wants `xdotool` on `PATH`, or `RAV_DEMO_GEOMETRY` set by hand.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |## Layout
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || Path | |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) ||---|---|
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `src/audio/` | cpal capture, and the macOS CoreAudio process tap — see [audio.md](audio.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `src/signal/` | FFT front end, bin-to-bar mapping, bar and cap ballistics |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `src/visual/` | the theme format: parsing, colours, the built-ins |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `src/ui/` | the `App` event loop and the analyser, scope and help widgets |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) || `themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Everything lives in the library — `src/main.rs` is argument parsing and wiring —
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |so the binary and the tests compile one copy of the tree rather than two that can
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |drift apart.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |## Releasing
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`release-plz` owns the version, `CHANGELOG.md`, the tag, the GitHub release and
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |the crates.io publish — one tool, reading the baseline from crates.io rather than
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |from a file in the repo.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Merging to master runs `release-plz release`, which publishes only if the version
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |in `Cargo.toml` is ahead of the registry. It then opens a Release PR proposing the
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |next version from the commits since the last tag; merging *that* PR is what
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |authorises the next publish. `semver_check` rejects a bump smaller than the API
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |change deserves.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |The previous setup was release-please plus a separate publish step, and it failed
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |in a way worth remembering: the version lived in
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`.release-please-manifest.json`, so when the publish step broke the repo kept
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |tagging releases that never reached the registry. rav has tags `rav-v0.2.0`
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |through `rav-v0.2.4` and has never existed on crates.io.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |Those tags have been deleted, along with their GitHub releases. A crate that is
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |publishable, carries release tags and has no crates.io row is a state release-plz
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |cannot resolve: it reads the next-version baseline from the registry and finds
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |nothing where the tags say there should be something. With the orphans gone the
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |registry and the repository agree — both say rav has never been released — and
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |`1.0.0-beta.1` is the first.
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |The rule that follows: never tag a release that has not reached the registry. The
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |publish step and the tag are one operation now, which is the point of using
| `crates/rav-appearance/themes/` | the bundled themes, TOML, compiled in — see [themes.md](themes.md) |release-plz rather than a tool that tags from a file it keeps itself.
