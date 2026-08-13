# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0-beta.11](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.10...rav-v1.0.0-beta.11) - 2026-08-13

### Added

- `v` bolts the analyser at an angle ([#116](https://github.com/i-am-logger/rav/pull/116))
- *(core)* geometry a skin can lean, turn and recede through ([#115](https://github.com/i-am-logger/rav/pull/115))

## [1.0.0-beta.10](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.9...rav-v1.0.0-beta.10) - 2026-08-12

### Documentation

- put the demo back on the crates.io page, and cut the sales pitch ([#111](https://github.com/i-am-logger/rav/pull/111))

## [1.0.0-beta.9](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.8...rav-v1.0.0-beta.9) - 2026-08-12

### Added

- the bar styles are shapes on pixels, not six labels over one picture ([#93](https://github.com/i-am-logger/rav/pull/93))
- rav can draw its bars as pixels ([#84](https://github.com/i-am-logger/rav/pull/84))

### Documentation

- say where a frame ends up, and what to do when it does not ([#105](https://github.com/i-am-logger/rav/pull/105))
- *(developing)* the module map was missing the pixel surface ([#100](https://github.com/i-am-logger/rav/pull/100))
- the README still said pixels were coming ([#99](https://github.com/i-am-logger/rav/pull/99))
- *(release)* say that the semver guard does not run during the beta ([#97](https://github.com/i-am-logger/rav/pull/97))
- say where the block shapes ended up ([#95](https://github.com/i-am-logger/rav/pull/95))
- say that rav.log holds every run, not just this one ([#88](https://github.com/i-am-logger/rav/pull/88))

### Fixed

- a terminal that stops measuring itself gets the picture taken down ([#109](https://github.com/i-am-logger/rav/pull/109))
- the cursor stops blinking on top of the bars ([#107](https://github.com/i-am-logger/rav/pull/107))
- --help stopped saying the surface flag draws nothing ([#103](https://github.com/i-am-logger/rav/pull/103))
- the panel stops saying it *would* draw once it is drawing ([#98](https://github.com/i-am-logger/rav/pull/98))
- rav opens as the skin its own preset names ([#96](https://github.com/i-am-logger/rav/pull/96))
- coarse and fine caps are two different pictures again ([#91](https://github.com/i-am-logger/rav/pull/91))

## [1.0.0-beta.8](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.7...rav-v1.0.0-beta.8) - 2026-08-11

### Added

- rav says what it is listening to, and plays something when it cannot hear ([#79](https://github.com/i-am-logger/rav/pull/79))
- a note has to light its bar while it is still sounding ([#77](https://github.com/i-am-logger/rav/pull/77))
- *(core)* a cap's fall is arithmetic an LED strip can reach ([#73](https://github.com/i-am-logger/rav/pull/73))
- ask the terminal whether it can draw images, and say so ([#71](https://github.com/i-am-logger/rav/pull/71))

### Documentation

- say what the code does, not what it was meant to do ([#74](https://github.com/i-am-logger/rav/pull/74))

### Fixed

- rav says why it will not start, and survives a crash ([#72](https://github.com/i-am-logger/rav/pull/72))

## [1.0.0-beta.7](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.6...rav-v1.0.0-beta.7) - 2026-08-11

### Added

- [**breaking**] separate what rav measures from what it looks like ([#66](https://github.com/i-am-logger/rav/pull/66))

## [1.0.0-beta.6](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.5...rav-v1.0.0-beta.6) - 2026-08-10

### Fixed

- *(audio)* stop the Linux capture falling behind real time ([#61](https://github.com/i-am-logger/rav/pull/61))

## [1.0.0-beta.5](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.4...rav-v1.0.0-beta.5) - 2026-08-10

### Performance

- *(nix)* build dependencies as their own derivation with crane ([#54](https://github.com/i-am-logger/rav/pull/54))

## [1.0.0-beta.2](https://github.com/i-am-logger/rav/compare/rav-v1.0.0-beta.1...rav-v1.0.0-beta.2) - 2026-08-09

### Fixed

- *(ci)* take the release tag from release-plz output, not git describe ([#44](https://github.com/i-am-logger/rav/pull/44))

## [1.0.0-beta.1](https://github.com/i-am-logger/rav/releases/tag/rav-v1.0.0-beta.1) - 2026-08-09

First release on crates.io.

### Added

- Real-time spectrum analyser with bars and peak caps, plus an oscilloscope view.
- Four built-in skins - rav, winamp, terminal and mono - cycled at runtime. A
  skin is a TOML file, so one can be added without touching the code.
- System audio capture with no virtual device: a CoreAudio process tap on macOS,
  a PipeWire or PulseAudio monitor source on Linux.

## [0.2.4](https://github.com/i-am-logger/rav/compare/rav-v0.2.3...rav-v0.2.4) (2025-08-09)


### Maintenance

* flake update and cleanup ([01f942d](https://github.com/i-am-logger/rav/commit/01f942ddc56ee23728f8f7ae63bdd3a1c8377481))
