---
phase: 2
title: "Launch always in Vietnamese"
status: completed
priority: P1
effort: "1h"
dependencies: []
---

# Phase 2: Launch always in Vietnamese

## Overview
`default_mode` persists the global VN/EN toggle, so one accidental ⌃⇧Space at
quit leaves the app launching disabled ("aa not work"). Make the global mode a
session-only toggle: always launch Vietnamese; keep exclusions/auto-fix/style
persisted.

## Requirements
- Functional: launch always Vietnamese; ⌃⇧Space toggles for the session only;
  exclusions/auto_fix/style still persist; old settings files still load.
- Non-functional: tolerant of the now-unknown `default_mode` key in old JSON.

## Architecture
Remove `default_mode` from `Settings` (serde ignores the unknown key in old
files). `Session::from_settings` no longer sets mode (stays default Vietnamese).
`snapshot()` no longer writes it.

## Related Code Files
- Modify: `crates/glowkey-engine/src/config.rs` (drop field + default)
- Modify: `crates/glowkey-engine/src/lib.rs` (from_settings, snapshot)
- Flip the user's current settings file to unblock immediately.

## Implementation Steps
1. Remove `default_mode` from `Settings`, `Default`, and any test refs.
2. `from_settings`: don't set `session.mode`.
3. `snapshot`: drop `default_mode`.
4. Update/adjust config + session tests.
5. Delete the stale `default_mode` from the live settings file.

## Success Criteria
- [x] Fresh + old settings load → session starts Vietnamese.
- [x] Toggling mode does not change the persisted file's activation on next launch.
- [x] Tests green, clippy clean.

## Risk Assessment
Low. Behavior change is intentional and matches Unikey/EVKey. Old files load via
serde unknown-field tolerance.
