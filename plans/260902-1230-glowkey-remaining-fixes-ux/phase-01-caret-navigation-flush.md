---
phase: 1
title: "Caret-navigation flush"
status: completed
priority: P1
effort: "1h"
dependencies: []
---

# Phase 1: Caret-navigation flush

## Overview
Arrow/Home/End/PageUp/PageDown keys move the caret without GlowKey's knowledge.
Today they fall to the boundary `commit()` arm (can emit a spurious auto-fix
restore at the moved caret) or leave a stale diff baseline. Treat them like the
Backspace/mouse paths: flush the engine and pass through.

## Requirements
- Functional: any caret-navigation key resets the composing buffer + clears the
  re-composition memory, then passes through untouched.
- Non-functional: no effect on normal typing; no new deps.

## Architecture
In `TapState::decide`, before the `unicode_char` match, detect caret-move
keycodes and return flush + Passthrough (mirrors the existing Delete handling).

## Related Code Files
- Modify: `app/src/tap.rs` (decide(): add `is_caret_move(keycode)` guard)

## Implementation Steps
1. Add const keycodes: Left 123, Right 124, Down 125, Up 126, Home 115,
   End 119, PageUp 116, PageDown 121.
2. `if is_caret_move(keycode) { session.flush(); return Passthrough; }`.
3. Real-CGEvent test: compose `hoo`, send Left arrow, then space → no restore
   emitted, engine idle.

## Success Criteria
- [x] Arrow keys flush; no spurious edit emitted mid-word.
- [x] Tests green, clippy clean.

## Risk Assessment
Very low. Keycodes are stable macOS virtual codes. If a code were wrong, the key
would behave as before (boundary/commit), not crash.
