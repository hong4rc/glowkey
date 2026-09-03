---
phase: 1
title: "VNI input method"
status: completed
priority: P1
effort: "2h"
dependencies: []
---

# Phase 1: VNI input method

## Overview
Add VNI as a second input method alongside Telex (Unikey/EVKey's other main
method). `viet65` → `việt`, tone digits 1–5, `6/7/8/9` for circumflex/horn/breve/đ.
The `vi` crate already ships `vi::VNI`, so this is a definition swap, not new logic.

## Requirements
- Functional: a Settings "Input method" segmented control (Telex / VNI); the
  engine renders per the choice; persists; live-switch takes effect next word.
- Non-functional: no regression to Telex; VNI covered by engine tests.

## Architecture
- `Settings.input_method: InputMethod { Telex, VNI }` (serde default Telex).
- Engine holds the method; `render()` picks `&vi::TELEX` or `&vi::VNI` for the
  `IncrementalBuffer`. `Session::set_input_method` re-flushes so the next word uses it.
- Settings window: segmented control mirrors the tone-style pattern.

## Related Code Files
- Modify: `crates/glowkey-engine/src/config.rs` (add `input_method`)
- Modify: `crates/glowkey-engine/src/lib.rs` (`InputMethod`, Engine method select,
  Session getter/setter, snapshot/from_settings)
- Modify: `app/src/tap.rs` (TapState accessors), `app/src/prefs_window.rs` (control)

## Implementation Steps
1. Add `InputMethod` enum (Serialize/Deserialize, Default = Telex) + `Settings` field.
2. Engine: store method; `render()` selects the `vi` definition; `set_input_method`.
3. Session/TapState wiring + persistence (from_settings, snapshot).
4. Settings segmented control "Input method: Telex | VNI".
5. Engine tests: `viet65`→việt, `dd`? (VNI `9`), tone digits; Telex tests unchanged.

## Success Criteria
- [x] VNI typing produces correct output; Telex unchanged; choice persists.
- [x] Tests green, clippy clean.

## Risk Assessment
Low. `vi::VNI` is a maintained definition. Risk: a user mid-word when switching —
mitigated by flushing on `set_input_method`. Signal it broke: wrong glyphs after a
switch → ensure flush happens before the next key.
