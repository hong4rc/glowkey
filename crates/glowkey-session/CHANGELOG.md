# Changelog

All notable changes to `glowkey-session` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `Session::pending_macro`: the macro the composing keys would expand into, asked before `commit` clears them — how a caller tells an expansion apart from an auto-fix restore.
- `Session::replace_macro`: rewrites one macro in place, for an edit that may rename the shortcut. Add-or-replace by shortcut could not express it, and left the old shortcut behind as a second row.

## [0.1.0] - 2026-09-05

First version carved out of the GlowKey application.

### Added
- `Session` and `SessionBuilder`: VN/EN mode, the frontmost `AppId`, word history, auto-fix, capitalisation, the correction hotkey's memory.
- `ExclusionList` with caller-supplied `ExclusionDefaults`; the `saved ∪ (defaults − removed)` merge and its tombstones.
- `Macro` with the UniKey table format, `WordOverride` and `WordPreference`.
- Re-exports `glowkey-engine`. Optional `serde` feature.
