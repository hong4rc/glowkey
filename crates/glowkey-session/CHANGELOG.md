# Changelog

All notable changes to `glowkey-session` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-09-05

First version carved out of the GlowKey application.

### Added
- `Session` and `SessionBuilder`: VN/EN mode, the frontmost `AppId`, word history, auto-fix, capitalisation, the correction hotkey's memory.
- `ExclusionList` with caller-supplied `ExclusionDefaults`; the `saved ∪ (defaults − removed)` merge and its tombstones.
- `Macro` with the UniKey table format, `WordOverride` and `WordPreference`.
- Re-exports `glowkey-engine`. Optional `serde` feature.
