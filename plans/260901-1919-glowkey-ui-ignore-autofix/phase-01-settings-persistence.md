---
phase: 1
title: "Settings & persistence"
status: completed
priority: P1
effort: "2-3d"
dependencies: []
---

# Phase 1: Settings & persistence

## Overview

A single `Settings` value that holds everything the UI controls — the ignore
list, auto-fix flag, placement style, default mode — and code to load it on
startup and save it whenever it changes. Pure Rust, fully testable.

## Requirements

**Functional**
- `Settings` struct: `exclusions: Vec<String>` (bundle ids), `auto_fix: bool`,
  `style: PlacementStyle`, `default_mode: InputMode`.
- Serialize/deserialize to JSON.
- `settings_store`: load from `~/Library/Application Support/GlowKey/settings.json`,
  save atomically (temp file + rename).
- Missing or corrupt file → return defaults (terminals/IDEs excluded, auto-fix on,
  new style, Vietnamese mode). Never crash.
- Build a `Session` from `Settings`, and snapshot a `Session`'s state back into
  `Settings` for saving.

**Non-functional**
- The engine crate stays platform-free: `Settings` + (de)serialization live in
  `glowkey-engine` and must compile and test on Linux. The *file path* and file
  I/O live in the app crate (`settings_store.rs`), which is macOS-only.

## Architecture

Split by platform boundary:

- `crates/glowkey-engine/src/config.rs` — `Settings` struct, `Default`, and
  JSON (de)serialization via a tiny hand-rolled format or `serde` if a dependency
  is acceptable. Prefer **no `serde`** to keep the engine dependency-light (the
  Windows shell inherits engine deps); a handful of fields serialize fine by hand,
  or use `serde` + `serde_json` scoped to the engine only if the team accepts it.
  Decide in step 1.
- `app/src/settings_store.rs` — resolves the Application Support path, reads/writes
  the file, calls the engine's (de)serialization.

`Settings` is the single source of truth the UI edits. Applying it to the live
`Session`: set style, set mode, replace the exclusion set.

## Related Code Files

- Create: `crates/glowkey-engine/src/config.rs`
- Modify: `crates/glowkey-engine/src/lib.rs` (expose `config`, a `Session::from_settings` / `Session::snapshot` pair)
- Create: `app/src/settings_store.rs`
- Modify: `app/src/main.rs` (load settings before building the tap)
- Modify: `app/src/tap.rs` (build `TapState`'s `Session` from loaded `Settings`; save on change)

## Implementation Steps

1. Decide serialization: hand-rolled vs `serde`. If `serde`, add it only to
   `glowkey-engine` and justify in a comment. (Recommendation: `serde` +
   `serde_json` — small, standard, and the JSON is user-inspectable.)
2. Write `Settings` with `Default` matching today's hardcoded behaviour.
3. Add `Session::from_settings(&Settings)` and `Session::snapshot() -> Settings`.
4. Write `settings_store`: path resolution (create the dir if absent), atomic
   save, tolerant load (corrupt → default, and log once).
5. Wire `main.rs`/`tap.rs`: load on startup, build the `Session`, and save after
   any settings mutation (a `TapState::save_settings()` helper).
6. Tests: round-trip `Settings` through JSON; corrupt input yields default;
   `from_settings`/`snapshot` are inverse for the fields they cover.

## Success Criteria

- [ ] `Settings` round-trips through JSON losslessly (unit test)
- [ ] Corrupt/missing file loads defaults without panicking (unit test)
- [ ] `from_settings` + `snapshot` preserve exclusions, auto-fix, style, mode
- [ ] Engine crate still compiles and tests on Linux
- [ ] App loads settings on startup and writes them on change (verified by a manual run: toggle something, check the file)

## Risk Assessment

**Adding `serde` to the engine widens its dependency graph** (inherited by a
future Windows shell). Signal: dependency bloat complaints. Response: it is small
and widely used; if it becomes a problem, the hand-rolled path is a contained
swap since serialization is isolated in `config.rs`.

**Assumption at risk:** that Application Support is always writable. Signal: save
fails. Response: log once, keep running with in-memory settings; the app still
works, it just won't persist — never crash.
