# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
