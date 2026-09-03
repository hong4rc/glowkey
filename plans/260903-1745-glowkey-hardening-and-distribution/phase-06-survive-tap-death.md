---
phase: 6
title: "Survive permission revocation and tap death"
status: pending
priority: P1
effort: "0.5d"
dependencies: []
---

# Phase 6: Survive permission revocation and tap death

## Overview

Raised by the owner during validation and confirmed by reading the code: if the
Accessibility switch is turned **off while GlowKey is running**, the app does not
notice.

There is no lag and no loop — that was the owner's question, and the answer is
clean. `accessibility_trusted()` is called at `tap.rs:1206` and at three sites
inside `wait_for_accessibility` (`:1298`, `:1355`, `:1363`), all of them in the
startup gate. Nothing re-checks afterwards: no `NSTimer`, no thread, and the
only `sleep` calls in the app crate are inside that gate. Revocation costs zero
CPU.

What it costs instead is silence. The tap dies, the process lives, the menu bar
keeps showing **VI**, Settings keeps opening, `glowkey.log` says nothing, and
Vietnamese simply stops. Re-granting the permission does not bring it back —
nothing re-enters the gate and nothing re-enables the port — so the only cure is
quit and relaunch, which the user has no way of knowing.

The status glyph asserting "VI" over a dead tap is what makes this a defect
rather than a limitation: the app's one indicator is lying.

## Requirements

- Functional: when the tap dies for any reason, the menu bar says so and the log
  records it.
- Functional: when the permission comes back, GlowKey recovers **without a
  relaunch**.
- Functional: a tap disabled by timeout (the other cause, today equally silent)
  is logged and counted, not just silently re-enabled.
- Non-functional: the health check must not cost measurable battery or add any
  per-keystroke work. It runs on a timer, never in the event path.
- Non-functional: no new modal that can appear behind the user's back. A dead
  tap changes the glyph and logs; the alert is reserved for the case where the
  user then clicks the menu.

## Architecture

**Detection.** `CGEventTapIsEnabled(port)` answers "is my tap alive" directly and
cheaply, and catches every cause at once — revocation, timeout disable, and the
system dropping the tap under load. Poll it from an `NSTimer` on the main run
loop every 2 seconds. That is two syscall-cheap checks per second against an
input method that already handles 20 keystrokes a second in the same loop; the
cost is not measurable, and Phase 3's latency instrumentation will confirm that
rather than assume it.

Distinguish the two states, because the fix differs:

| `tap_is_enabled` | `accessibility_trusted` | Meaning | Action |
|---|---|---|---|
| true | — | Healthy | Nothing |
| false | true | Disabled by timeout or load | Re-enable in place; log it with a count |
| false | false | Permission revoked | Glyph → warning, log once, and re-create the tap when trust returns |

**Recovery.** On regained trust, the existing port is worthless — the tap was
created under a grant that no longer exists. Re-run the port creation from
`run()` (extracted into a `create_tap(ctx) -> Option<CFMachPort>` helper so the
timer and startup share one path), install the new run-loop source, and replace
the port in `TapContext`. The leaked `TapState` is untouched, so every setting,
exclusion and macro survives; only the tap is rebuilt.

**Telling the truth.** A third glyph state alongside VI and EN — the project
already has the precedent in the "VI ⚠" HUD variant from the session-only
terminal un-exclusion. The menu gains a line naming the cause and offering
"Open System Settings", reusing the string and the button from the startup gate.

## Related Code Files

- Modify: `app/src/tap.rs` — extract `create_tap`; add the health timer; log and
  count the `TapDisabledByTimeout | TapDisabledByUserInput` branch at
  `:1147-1158`, which today re-enables blind with no return check and no log line.
- Modify: `app/src/menu_bar.rs` — the third glyph state and the menu line.
- Modify: `app/src/strings.rs` — both languages for every new string.
- Modify: `docs/handoff.md` §6 (new entry) and §7 (the new log lines).
- Create: `docs/decisions/0007-tap-health-monitor.md` — why a 2-second poll
  rather than a TCC notification (there is no public API to observe an
  Accessibility grant changing; polling is the only supported route).

## Implementation Steps

1. **Reproduce first.** Run GlowKey, revoke the switch, and record what actually
   happens: does the process survive, does the callback fire with a
   `TapDisabled*` event, does `tap_is_enabled` go false? Some macOS versions kill
   the app outright, which would make most of this phase unnecessary — that is a
   result worth having before writing code.
2. Extract `create_tap` out of `run()`; startup calls it and behaves as before.
3. Add the health timer with the three-state table above; log every transition,
   never per tick.
4. Add the glyph state and the menu line, both languages.
5. Verify recovery by hand: revoke → glyph changes and the log says so → re-grant
   → Vietnamese types again **without a relaunch**.
6. Add the timeout case to the log with a running count, so a tap that flaps
   under load is visible in `glowkey.log` instead of invisible.
7. Add the whole sequence to `docs/manual-verification.md` (Phase 5).

## Success Criteria

- [ ] Step 1's reproduction is recorded in this file under "Outcome", including
      the case where macOS simply terminates the app
- [ ] Revoking the permission changes the menu bar glyph within ~2 seconds
- [ ] `glowkey.log` names the cause, once per transition, never per tick
- [ ] Re-granting restores typing with no relaunch
- [ ] A timeout-disabled tap is logged and counted
- [ ] Idle CPU is unchanged to the precision Activity Monitor shows

## Risk Assessment

- **macOS may terminate the app on revocation, making this phase moot.** *Signal:*
  step 1 shows the process gone. *Response:* keep only the timeout logging and
  the honest glyph, drop the recovery path, and record why in decision 0007.
  Do not build recovery for a state that cannot occur.
- **Re-creating a tap while the old one is half-alive.** Two live taps process
  every keystroke twice — the same failure the two app identities exist to
  prevent (`docs/handoff.md` §8). *Response:* disable and drop the old port
  before creating a new one, and assert in the log that exactly one port exists
  at a time.
- **A 2-second timer that wakes an idle machine.** Cheap, but not free, and this
  is a background agent that runs all day. *Signal:* measurable idle CPU or
  battery impact. *Response:* back off to 5 seconds when the tap has been healthy
  for a while; the user only needs to learn the tap died within a few seconds of
  trying to type, not instantly.
- **The glyph change becomes noise.** If the tap flaps under load, a glyph that
  flickers is worse than one that lies. *Response:* only show the warning state
  after two consecutive failed checks.
