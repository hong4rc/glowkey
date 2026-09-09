---
phase: 4
title: "Windows hook liveness watchdog"
status: pending
priority: P0
effort: "1-2d code + one desktop sitting"
dependencies: []
---

# Phase 4: Windows hook liveness watchdog

Executes [`260905-1643` §3.2](../260905-1643-glowkey-production-hardening/phase-3-correctness.md)
(A3, P0). That section is the design of record.

## Overview

Windows silently removes a low-level hook whose callback is too slow. When it
does, the tray keeps showing **VI**, typing stops transforming, and nothing
anywhere says why. macOS has `platform/macos/health.rs` for exactly this
(`decisions/0007`); Windows has nothing.

Comes before phase 5 because both touch `hook.rs`, and this one is the smaller
of the two.

## Requirements

- Functional: when Windows removes the hook, the indicator reports `HookGone`
  and the user is offered a reinstall.
- Functional: no false positive during normal use — including a machine left
  idle, a full-screen game, a locked session, and a laptop resuming from sleep.
- Non-functional: the check runs on the existing message loop. It must not add
  work to the hook callback, which is the thing whose slowness causes the
  problem in the first place.
- Non-functional: **the watchdog is not shipped on reasoning alone.** §3.2 says
  so explicitly, and it is the phase's hard gate.

## Architecture

**Current state, confirmed:** `HOOK` is zeroed only by `uninstall()`, so
`Indicator::HookGone` is reachable only when GlowKey itself removed the hook —
never in the case that matters. `hook.rs:185` now says this plainly (commit
`5ce4c0c`, §3.2's "done" half) rather than describing a guard that does not
exist; `is_installed()` returns `true` after Windows has silently detached the
hook.

**The detection, from §3.2:** a message-loop timer comparing the last callback
tick against `GetLastInputInfo`. Input happened with no callback ⇒ the hook is
gone ⇒ `HookGone` ⇒ offer reinstall. `shell::reinstall_hook` already exists, so
the remedy is wired; only the detection is missing.

**Borrow the macOS constants rather than inventing new ones.** `health.rs`
already encodes the tuning this problem needs, and its shape is the argument
against a naive one-shot check:

| `health.rs` | Value | Why it exists here too |
|---|---|---|
| `HEALTH_CHECK_SECONDS` | 2.0 | Timer period |
| `HEALTH_SKIP_AFTER_KEYSTROKE` | 3s | Do not judge health immediately after real input |
| `HEALTH_FAILURES_BEFORE_WARNING` | 2 | **One missed tick is not a dead hook** — this is the false-positive defence |
| `HEALTH_FAILURES_BEFORE_GIVING_UP` | 30 | Stop retrying forever |
| `HEALTH_FAILURES_PER_LOG_LINE` | 30 | Do not flood the log |

Consecutive-failure counting is the mechanism that makes this safe. A single
comparison between a tick and `GetLastInputInfo` will disagree constantly under
normal conditions.

**What `GetLastInputInfo` actually reports, and the trap:** it is system-wide
and includes mouse movement, and it does *not* count injected input from a
non-elevated process the same way as real input. Two consequences:

1. Mouse movement raises the input timestamp with no keyboard callback at all —
   the single largest false-positive source. Compare against *keyboard* activity
   or accept that mouse-only activity must not count as a missed callback.
2. GlowKey's own injected keystrokes must not be read as user input, or the
   watchdog measures itself.

**Interaction with elevation, which is already honest.** `BlockedByElevation`
was seen live on 2026-09-06 (`#17308`: `REACH … cannot receive injected
input`). That is a *different* reachability failure and it already reports
correctly. The watchdog must not fire on top of it — when the foreground window
is elevated, the callback legitimately sees nothing, and reinstalling the hook
fixes nothing. Check elevation state before concluding the hook is dead.

Same for a locked session and a UAC prompt: input goes to a different desktop,
the callback correctly sees nothing, and reinstalling is wrong.

## Related Code Files

- Modify: `app/src/platform/windows/hook.rs` — record the last-callback tick;
  the timer lives on `run_message_loop` (~:305). Correct the `is_installed`
  doc comment (~:180-192) once a real check exists — it currently states none does
- Modify: `app/src/platform/windows/shell.rs` — wire `HookGone` to the existing
  `reinstall_hook`
- Modify: `app/src/platform/windows/indicator.rs` — `HookGone` becomes reachable
  for the real cause
- Read only: `app/src/platform/macos/health.rs` — the model, and the constants
- Read only: `app/src/platform/windows/elevation.rs` — the elevation state to
  gate on

## Implementation Steps

1. Record the last-callback tick in the hook callback. One atomic store of a
   tick count — nothing more, since callback cost is the root cause.
2. Add a `SetTimer` on the existing message loop at the `health.rs` period.
   Do **not** spawn a thread; the loop is already there.
3. Implement the comparison with all four suppressions: recent keystroke,
   mouse-only activity, elevated foreground window, locked session / different
   desktop.
4. Add consecutive-failure counting with the `health.rs` thresholds. Warn at 2,
   give up at 30, one log line per 30.
5. On confirmed death: `HookGone`, log the reason, offer `reinstall_hook`.
   Rate-limit reinstall attempts — a reinstall loop is the named risk.
6. **Build the deliberately-slow callback.** A debug-only build (feature flag or
   `cfg`) whose callback sleeps past the Windows timeout, so a genuinely dead
   hook can be produced on demand. This is the validation instrument and step 7
   cannot happen without it.
7. **Live run on an idle machine.** An agent can drive this — the earlier
   "`SendInput` is denied to an agent" claim was refuted on 2026-09-06 (it was
   UIPI from an elevated foreground window; see `docs/manual-verification-windows.md`
   and `scripts/probe-sendinput.ps1`). What is required instead: **ask first**,
   and have the machine to yourself, because these runs take over the screen and
   type. Keep an ordinary, non-elevated window in the foreground. Run:
   - slow-callback build → hook dies → watchdog detects, reports, offers reinstall
   - reinstall → typing works again
   - normal build, 10 minutes of ordinary typing → no false positive
   - idle 10 minutes, then type → no false positive
   - mouse-only activity for 2 minutes → no false positive
   - lock and unlock → no false positive
   - sleep and resume → no false positive
   - elevated window focused → `BlockedByElevation`, not `HookGone`
8. `cargo test` + `cargo clippy`.

## Success Criteria

- [ ] A deliberately-slow-callback build reproduces a dead hook on demand
- [ ] The watchdog detects that death and offers a reinstall that restores typing
- [ ] Zero false positives across all six normal-use scenarios in step 7
- [ ] An elevated foreground window still reports `BlockedByElevation`
- [ ] Reinstall attempts are rate-limited; no loop is reachable
- [ ] The hook callback gained at most one atomic store
- [ ] `hook.rs`'s `is_installed` doc comment no longer says no liveness check exists
- [ ] The verification run is recorded in `plans/reports/`

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| **False positive → reinstall loop.** §3.2's named risk | Repeated reinstall log lines during ordinary use | Consecutive-failure counting plus all four suppressions, validated by step 7's six scenarios. Rate-limit reinstalls independently, so even a detection bug cannot loop |
| Shipping the watchdog without ever seeing a real dead hook | Step 6 is skipped as "hard to build" and step 7 runs only the no-false-positive half | Hard gate: §3.2 says do not ship on reasoning alone. Without step 6 this phase is not done, however clean the code reads |
| `GetLastInputInfo` counting mouse movement as evidence of a missed callback | False positives concentrated during mouse-only work | Explicit mouse-only suppression, with its own scenario in step 7 |
| The watchdog measures GlowKey's own injected input | Detection never fires, or fires only when idle | Injected input must not raise the comparison baseline; verify against the slow-callback build, where the answer is known |
| Fires on top of elevation, prompting a useless reinstall | `HookGone` where `BlockedByElevation` is correct | Gate on `elevation.rs` before concluding death; scenario in step 7 |
| The timer's work drifts into the callback over time | Typing latency regresses; hook removed for slowness — the original bug | Keep the callback to one atomic store. Any future logic goes on the timer, never the callback |
