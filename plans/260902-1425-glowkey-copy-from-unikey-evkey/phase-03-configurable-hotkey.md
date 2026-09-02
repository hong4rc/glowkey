---
phase: 3
title: "Configurable toggle hotkey"
status: completed
priority: P2
effort: "3h"
dependencies: []
---

# Phase 3: Configurable toggle hotkey

## Overview
Unikey/EVKey let the user pick the VN/EN toggle hotkey. GlowKey hard-codes
⌃⇧Space (mode) and ⌃⇧E (per-app). Make at least the mode toggle user-selectable.

## Requirements
- Functional: choose the mode-toggle hotkey from a small preset list (⌃⇧Space,
  ⌃Space, ⌥Space, ⌃⇧Z…); persists; the tap honors it.
- Non-functional: presets only (a full recorder is out of scope); no conflict with
  the per-app ⌃⇧E; test the match predicate.

## Architecture
- `Settings.toggle_hotkey: HotkeyPreset` (enum, default CtrlShiftSpace).
- `is_toggle_hotkey(flags, keycode)` reads the configured preset instead of a
  hard-coded combo. Preset → (modifier mask, keycode).
- Settings popup (NSPopUpButton) lists the presets.

## Related Code Files
- Modify: `crates/glowkey-engine/src/config.rs` (HotkeyPreset)
- Modify: `app/src/tap.rs` (`is_toggle_hotkey` reads preset via TapState)
- Modify: `app/src/prefs_window.rs` (NSPopUpButton) — needs `NSPopUpButton` feature

## Implementation Steps
1. Define `HotkeyPreset` enum + mapping to (mask, keycode); add setting.
2. Route `is_toggle_hotkey` through the configured preset.
3. Settings popup; persist on change; live-apply.
4. Tests: each preset's `is_toggle_hotkey` matches only its combo (real CGEvents).

## Success Criteria
- [ ] Selected hotkey toggles mode; others don't; choice persists.
- [ ] Tests green, clippy clean.

## Risk Assessment
Medium. A chosen combo could clash with a system/app shortcut; presets are curated
to avoid the worst. Signal it broke: toggle fires on unrelated keys → tighten the
preset's modifier mask match.
