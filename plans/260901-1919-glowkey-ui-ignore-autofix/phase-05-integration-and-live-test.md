---
phase: 5
title: "Integration & live test"
status: pending
priority: P1
effort: "2-3d"
dependencies: [4]
---

# Phase 5: Integration & live test

## Overview

Tie the pieces together, run the whole thing on real apps, and confirm every
success criterion on a live machine — the parts that only a running session can
verify.

## Requirements

- All four prior phases wired: settings load at startup → build session; menu and
  prefs mutate the session and save; auto-fix runs at the boundary.
- A full live pass on the real app matrix.
- Docs and checkpoint updated; CI privacy guard still green.

## Architecture

No new components — this phase is integration and verification. Confirm the data
flow end to end: a change in the menu or prefs updates the `Session`, writes
`settings.json`, and the very next keystroke reflects it; a restart reloads it.

## Related Code Files

- Modify: `app/src/main.rs` — final wiring order (load settings → tap → menu)
- Modify: `docs/checkpoint.md` — current state, install/run, on-device watch-list
- Modify: `README.md` — features (menu bar, per-app control, auto-fix)
- Modify: `.github/workflows/ci.yml` — only if new checks are warranted

## Implementation Steps

1. Finalize startup order and shared-state ownership.
2. Live matrix (grant Accessibility, run, type):
   - Menu bar shows correct state; toggling current app works in Slack, Chrome,
     TextEdit; excluded apps (Terminal/iTerm) stay raw.
   - Preferences: add/remove an app, watch behaviour change; flip auto-fix and
     style; relaunch and confirm persistence.
   - Auto-fix: `exit` → `exit`; `hoongf` → `hồng`; a few real words unaffected.
   - The Chrome `hoồng` emit path (open question 3) — confirm it types cleanly now
     that this phase exercises it heavily; if not, that is a separate fix.
3. Run the full test suite and clippy; confirm the privacy guard (no networking
   framework) still passes.
4. Update `docs/checkpoint.md` and `README.md`.

## Success Criteria

- [ ] Every `plan.md` success criterion verified on a live machine
- [ ] Menu bar per-app toggle works across Slack, Chrome, TextEdit; terminals stay excluded
- [ ] Preferences add/remove/options all work and persist across relaunch
- [ ] Auto-fix behaves (`exit` restored, real Vietnamese untouched)
- [ ] Full `cargo test` + clippy green; engine still builds on Linux
- [ ] Privacy guard passes (no networking framework linked)
- [ ] `docs/checkpoint.md` and `README.md` current

## Risk Assessment

**Live testing surfaces an AppKit issue only visible on screen.** Signal: a
control does nothing, or the menu label is wrong. Response: this is expected for
GUI work that cannot be unit-tested; iterate with the run-and-look loop, using
`log stream` for state and keeping actions logged behind `GLOWKEY_DEBUG`.

**Assumption at risk:** that the Chrome delivery bug is resolved by the
session-posting change. Signal: `hoồng` still appears. Response: it is independent
of this plan's features; if it persists, capture the `GLOWKEY_DEBUG` emit log and
fix the emit path separately — do not let it block shipping the UI/ignore/auto-fix
work, which is orthogonal.

**Scope creep at the finish.** Signal: "just one more preference." Response: v1 is
the stated goals; anything else (allow-list, macros, hotkey editing) is a v2
conversation per the plan's non-goals.
