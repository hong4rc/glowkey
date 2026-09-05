---
phase: 1
title: "Carve the engine into modules, no behaviour change"
status: completed
priority: P1
effort: "4h"
dependencies: []
---

# Phase 1: Carve the engine into modules, no behaviour change

## Overview
Split the 2,209-line `lib.rs` into modules along the three layers, keeping
every public path working through `pub use`. Pure move; the diff is
reviewable as "same code, new files".

## Requirements
- Functional: identical behaviour; all 12 integration test files pass without
  edits.
- Non-functional: `#![warn(missing_docs)]` on the crate and the warnings fixed;
  `cargo public-api` (or a manual list) shows the same public items before and
  after.

## Architecture
```
crates/glowkey-engine/src/
  lib.rs          docs + pub use only
  method.rs       InputMethod, PlacementStyle, SIMPLE_TELEX definition, Strategy selection
  engine.rs       Engine, KeyResponse, BackspaceOutcome, BoundaryBackspace
  tones.rs        remove_tones and tone-mark helpers
  macros.rs       Macro, MacroConflict, parse_table/format_table (moves out in phase 3)
  overrides.rs    WordOverride, WordPreference, lookup (moves out in phase 3)
  session.rs      Session, InputMode, ExclusionToggle (moves out in phase 3)
  exclusion.rs    unchanged (moves out in phase 3)
  config.rs       Settings (moves out in phase 2)
  hotkey.rs       HotkeyPreset (moves out in phase 2)
  language.rs     Language (moves out in phase 2)
```
No item changes visibility or name. `lib.rs` keeps `pub use` for everything
`app/` and `glowkey-input` import today.

## Related Code Files
- Create: the module files above
- Modify: `crates/glowkey-engine/src/lib.rs`
- Delete: none

## Implementation Steps
1. Record the public API: `cargo doc --no-deps -p glowkey-engine` and save the
   item list to `plans/reports/engine-public-api-before.md`.
2. Move code block by block into the modules; `pub use` from `lib.rs`.
3. Add `#![warn(missing_docs)]`; write the missing one-line docs.
4. `cargo test --workspace`; `cargo clippy` on the three targets.
5. Diff the public API list; it must be identical.

## Success Criteria
- [x] All tests pass with zero test-file edits.
- [x] Public API list identical before and after.
- [ ] No file over 600 lines in the engine crate. **Deferred to phase 3:** `session.rs` is 1,156 lines and leaves the crate there, where it gets `builder.rs`, `corrections.rs`, `macros.rs`; `engine.rs` is 613, split in phase 5 if it still bothers.
- [x] Clippy and doc warnings zero.

<!-- Updated: Validation Session 1 - macros/overrides leave the core in phase 3 -->

## Risk Assessment
- Private helpers shared across the new modules need `pub(crate)`; that is the
  only expected edit beyond moves. If a helper is used by both `Engine` and
  `Session`, it belongs to `engine.rs` and `Session` calls it through `Engine`.
