---
phase: 3
title: "Session crate and injected defaults"
status: pending
priority: P1
effort: "6h"
dependencies: [2]
---

# Phase 3: Session crate and injected defaults

## Overview
Lift `Session`, `InputMode`, `ExclusionList` and `ExclusionToggle` into
`crates/glowkey-session`, built through a builder, with the shipped exclusion
defaults injected by the app. The engine crate is then the pure core.

## Requirements
- Functional: the five `Session` test files move to the new crate and pass
  unchanged apart from `use` lines; `glowkey-input`'s ladder tests pass
  unchanged.
- Non-functional: `glowkey-session` has no platform names in it: no `.exe`, no
  bundle id, no `terminal` list. `AppId` is an opaque newtype.

## Architecture
```rust
// crates/glowkey-session/src/lib.rs
pub struct AppId(String);                       // bundle id or process name; opaque here
pub struct SessionBuilder { .. }
impl Session {
    pub fn builder() -> SessionBuilder;
}
impl SessionBuilder {
    pub fn style(self, PlacementStyle) -> Self;
    pub fn input_method(self, InputMethod) -> Self;
    pub fn exclusions(self, ExclusionList) -> Self;
    pub fn auto_fix(self, bool) -> Self;        // … one per policy knob
    pub fn macros(self, Vec<Macro>) -> Self;
    pub fn word_overrides(self, Vec<WordOverride>) -> Self;
    pub fn build(self) -> Session;
}
impl ExclusionList {
    pub fn with_defaults<I: IntoIterator<Item = AppId>>(defaults: I) -> Self;
    pub fn from_saved(saved, removed_defaults, defaults) -> Self;
}
```
- `DEFAULT_EXCLUSIONS` moves to `app/src/default_exclusions.rs` with its
  per-platform tables (the Windows `.exe` list and the macOS bundle ids already
  differ); the app passes the right one to `with_defaults`.
- `Session::set_frontmost_app(impl Into<String>)` becomes
  `set_frontmost_app(AppId)`; the shells construct `AppId` from what they know.
- `glowkey-input` depends on `glowkey-session` (for `Session`) and
  `glowkey-engine`; `app` depends on all three.
- The engine keeps `Macro`/`WordOverride` as data types (they are text
  policy, not product), re-exported by the session crate.

## Related Code Files
- Create: `crates/glowkey-session/{Cargo.toml,src/lib.rs,src/session.rs,src/exclusion.rs,src/builder.rs,tests/*}`, `app/src/default_exclusions.rs`
- Modify: `Cargo.toml` (workspace member), `crates/glowkey-engine/src/lib.rs`
  (remove session, exclusion), `crates/glowkey-input/Cargo.toml` and `src`,
  `app/**` imports, `app/src/session_adapter.rs`, `.github/workflows/ci.yml`
  (Linux job adds `-p glowkey-session`)
- Delete: `crates/glowkey-engine/src/{session,exclusion}.rs`

## Implementation Steps
1. Create the crate; move `session.rs`, `exclusion.rs`, the five test files.
2. Builder; `from_settings` is gone since phase 2, so the adapter calls the
   builder.
3. `ExclusionList::with_defaults`; move the table to the app; delete the
   constant from the crate.
4. `AppId`; update both shells' `set_frontmost_app` call sites (macOS:
   `dispatch.rs`, `mod.rs`; Windows: `hook.rs`, `shell.rs`).
5. `glowkey-input` points at the session crate.
6. CI: Linux job builds and tests the three library crates.
7. Gates on three targets.

## Success Criteria
- [ ] `grep -rn "\.exe\|com\.\|terminal" crates/glowkey-session/src` is empty.
- [ ] `cargo test -p glowkey-session` green with test files moved, not edited
      beyond `use`.
- [ ] `glowkey-input` tests unchanged and green.
- [ ] Linux CI job covers all three library crates.

## Risk Assessment
- `Session` uses `Engine` internals (`restore`, `raw_vec`) that may currently
  be `pub(crate)`. Anything the session needs becomes `pub` on `Engine` with a
  doc comment; if that exposes something unsafe to hand a consumer, the
  boundary is wrong and the helper moves up. Signal: a `pub(crate)` that
  cannot be made public cleanly.
- The exclusion tombstone rule (`saved ∪ (defaults − removed)`) is tested in
  `exclusion.rs`; the injected defaults must keep it. Tests move with it.
