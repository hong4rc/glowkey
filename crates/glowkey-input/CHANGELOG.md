# Changelog

All notable changes to `glowkey-input` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-09-05

First version carved out of the GlowKey application.

### Added
- `decide`, the decision ladder, with `Decision` and `Effects`.
- `KeyEvent`, `Key`, `Modifiers`.
- `hotkey`: presets, matching, and recording (`capture`).
- `Platform` and `handle`: the port a shell implements and the one call it makes per key.
