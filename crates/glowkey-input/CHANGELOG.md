# Changelog

All notable changes to `glowkey-input` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed
- A macro fired by **Tab** now consumes the Tab instead of replaying it: the key was the trigger, and replaying it moved the focus or accepted a completion the moment the expansion landed. A space still types, and a Tab after an auto-fix restore is still replayed.

## [0.1.0] - 2026-09-05

First version carved out of the GlowKey application.

### Added
- `decide`, the decision ladder, with `Decision` and `Effects`.
- `KeyEvent`, `Key`, `Modifiers`.
- `hotkey`: presets, matching, and recording (`capture`).
- `Platform` and `handle`: the port a shell implements and the one call it makes per key.
