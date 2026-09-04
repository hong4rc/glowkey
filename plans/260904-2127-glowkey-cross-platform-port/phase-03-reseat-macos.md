---
phase: 3
title: "Re-seat macOS on the neutral layer"
status: complete
priority: P1
effort: "2d"
dependencies: [1, 2]
---

# Phase 3: Re-seat macOS on the neutral layer

## Overview

Make the existing macOS app a *client* of `glowkey-input` rather than the owner of
the policy, moving `app/src/tap/` to `app/src/platform/macos/`. Zero behaviour
change; all 194 tests stay green. This is the phase that proves the neutral layer is
actually sufficient before any Windows code exists.

## Requirements

- Functional: no observable behaviour change whatsoever.
- Functional: every existing test passes, unmodified where possible.
- Non-functional: the macOS adapter shrinks to translation — `CGEvent` in,
  `KeyEvent` out; `Decision` in, `CGEventPost` out.

## Architecture

```text
app/src/platform/macos/
  mod.rs        TapState, run, the C callback, circuit breaker   (was tap/mod.rs)
  adapt.rs      CGEvent → KeyEvent, macOS keycodes → Key         (was tap/keys.rs)
  emit.rs       Decision → CGEventPost, the omnibox guard        (unchanged)
  health.rs     the tap health monitor                           (unchanged)
  permission.rs the Accessibility gate                           (unchanged)
  settings.rs   the *_and_save accessor wall                     (unchanged)
  tests.rs      CGEvent-driven adapter tests                     (thinned)
```

`decide.rs` disappears: its body is now `glowkey_input::decide`, and what remains at
the tap is the call plus the macOS-only steps that genuinely are macOS — the
self-event tag check, the circuit breaker, and `try_borrow` guarding.

The **tests split by what they prove**. Policy tests (the ladder, re-composition,
the cap, boundary handling) moved to `glowkey-input` in Phase 1. What stays here is
what needs a real `CGEvent`: the tag guard, suppression versus passthrough at the
`CGEventPost` level, and the hotkey recorder.

## Related Code Files

- Move: `app/src/tap/*` → `app/src/platform/macos/*`
- Delete: `app/src/tap/decide.rs` (body now lives in `glowkey-input`)
- Modify: `app/src/main.rs`, `app/src/menu_bar.rs`, `app/src/main_menu.rs`,
  `app/src/prefs/*` — import paths only
- Create: `app/src/platform/mod.rs` — `#[cfg(target_os = "macos")] pub mod macos;`

## Implementation Steps

1. Create `app/src/platform/mod.rs` with the cfg gate, and move the directory.
2. Write `adapt.rs`: the macOS virtual key code table (51 Delete, 117 forward-delete,
   53 Escape, 49 Space, 14 E, 13 W, 6 Z, 123-126/115/116/119/121 caret moves) mapped
   to `Key`, and `CGEventFlags` mapped to `Modifiers`.
3. Replace the body of `TapState::decide` with a call into `glowkey_input::decide`.
4. Run the full suite. Anything that fails is a real difference — investigate it,
   never adjust the test.
5. Re-read `docs/manual-verification.md` §2, §4, §5 and confirm by inspection that
   the ladder's observable steps are unchanged.

## Success Criteria

- [x] All 194 tests green, clippy silent, `cargo fmt` introduces no new drift
- [x] `app/src/platform/macos/` contains no policy — only translation and macOS I/O
- [x] `docs/handoff.md` §3 updated for the new layout
- [x] `docs/decisions/0008`'s rule still holds: no blocking call reaches the callback
- [ ] Release build runs and types Vietnamese (a human check, one minute) — the
      bundle builds and signs (`scripts/build-app.sh`); the typing is still owed

## Risk Assessment

**A refactor this wide can pass every test and still be wrong**, because 160 of the
194 tests are engine tests that never touch this code. The real coverage here is the
34 tap tests plus a human typing. *Signal:* none automatic — that is the point.
*Response:* treat the one-minute human check as mandatory, not optional, and do it
before Phase 4 starts.

**Import churn across ~20 files hides a real change in noise.** *Signal:* the diff
is large and mostly `use` lines. *Response:* commit the move and the rewiring
separately, so `git log --follow` and review both stay tractable.

**The circuit breaker and tag guard are macOS-shaped but conceptually shared.**
*Signal:* Phase 4 wants to duplicate them. *Response:* leave them here for now;
after Windows has its own, extract the common shape if and only if they turn out
identical. Duplicating twice then extracting beats guessing once.
