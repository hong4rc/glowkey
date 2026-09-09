---
phase: 5
title: "Verify the Chromium omnibox guard"
status: pending
priority: P0
effort: "half a day on an idle machine"
dependencies: []
---

# Phase 5: Verify the Chromium omnibox guard

Closes out [`260905-1643` §3.1](../260905-1643-glowkey-production-hardening/phase-3-correctness.md)
(A1, P0). That section is the design of record.

## Overview

**This phase writes almost no code — §3.1's real fix has already landed on this
branch.** §3.1 is written as five open implementation steps; all five are done.
What is left is the live acceptance it was never given, and the one fact no unit
test can supply: whether the *real* Edge/Chrome omnibox matches the UIA class the
code keys on.

Verified present in the source before this phase was written:

| §3.1 step | State | Where |
|---|---|---|
| 1. Widen registration to `EVENT_OBJECT_FOCUS`, filter event-aware | done | `foreground.rs:157-158`, event-aware filter at `:233-238` |
| 2. Worker thread, bounded channel, drops rather than blocks | done | `omnibox.rs:122` `start()`, pattern from `hook_log.rs` |
| 3. Worker sets `FOCUS_IS_OMNIBOX` | done | `omnibox.rs:76`, `:149`, `:157` |
| 4. `needs_omnibox_guard` gains the condition, re-wired into `edit_inputs` | done | `inject.rs:250-252` (all three conditions), called at `:119` |
| 5. Clear the flag on every foreground change | done | `omnibox.rs:106` `note_focus_changed()`, test `a_focus_change_fails_safe` at `:302` |

The forward-delete is genuinely re-enabled: `inject.rs:119-121` pushes
`VK_DELETE` down and up when the guard returns true, and the tests pin both
directions (`:306-318` with the flag forced on, `:283-293` with it off).

So the interim disable is gone and the real guard is live — **unverified against
a real browser.**

## Requirements

- Functional: §3.1's stated acceptance, unchanged — the address bar gives
  `hồng`; a Gmail draft mid-paragraph loses no character; `EMIT took=` shows no
  new millisecond-scale cost; no `LowLevelHooksTimeout`.
- Functional: `OMNIBOX_CLASS` matches what Edge and Chrome actually report.
- Non-functional: any code change is a correction to a measured mismatch, not a
  redesign. If the design is wrong, that reopens §3.1 rather than being patched
  here.

## Architecture

**The one thing unit tests cannot cover.** `omnibox.rs:317`'s test
`the_omnibox_class_is_the_chromium_view_name` pins the *constant*; it cannot
confirm the constant is right. `focused_class()` reads
`UIA_ClassNamePropertyId` and compares to `OMNIBOX_CLASS` (`:149`). If the real
class differs — by browser, by version, or by Edge vs Chrome — the flag stays
`false`, the guard never fires, and the failure is **silent and looks exactly
like success**: no forward-delete, no crash, and the `hoồng` mis-render simply
persists. That is the specific thing this phase measures.

**Both instruments already exist — do not rebuild them.** Found by verification
2026-09-06; an earlier draft of this phase wrote these as manual instructions:

| Script | What it does | Which step |
|---|---|---|
| `scripts/probe-omnibox-identity.ps1` | Prints the UIA identity of whatever you focus. Its own header says it was written for §3.1 and that the output "decides the predicate" — and that the property must not be the element *name*, which is localized and differs between Chrome and Edge | step 3 |
| `scripts/verify-windows-omnibox.ps1` | Drives a real Edge window and types into the address bar, reading the result back through UIA so the answer is code points rather than a squint at a screenshot. Sends no Enter; Escape restores the URL | step 4 |

`verify-windows-omnibox.ps1` carries the limitation to plan around, in its own
header: **focusing the address bar is unreliable while a page has grabbed the
keyboard.** A YouTube player swallowed `Ctrl+L`, `Alt+D` and `F6` across several
attempts. Open a plain tab (`about:blank`) first, or focus the address bar by
hand and comment out the focus block. Its first line is
`RUN THIS ONLY WHEN YOU ARE NOT USING THE MACHINE.`

The wider `scripts/verify-windows-tier*.ps1` suite covers the surrounding
regression checks; prefer extending one of those over writing a new script.

**What the guard must satisfy, from the live evidence in §3.1.** The 2026-09-06
session established that only the *first* word into a freshly cleared omnibox
mis-rendered — the inline-autocomplete signature — while the five after it were
correct. So the acceptance run must reproduce that exact condition (`Ctrl+A`,
Backspace, then type) rather than typing repeatedly into a dirty field, which
passes trivially.

**Open question worth settling here, not later.** Because only the
freshly-cleared-first-word case fails, a fix keyed to *that* condition might not
need UIA at all. The UIA machinery is built and working, so this is not a reason
to change anything now — but if verification shows the class match is fragile
across browsers or versions, that cheaper condition is the fallback to compare
against rather than hardening the class matching indefinitely.

## Related Code Files

- Read only: `app/src/platform/windows/omnibox.rs`, `inject.rs`, `foreground.rs`
- Modify **only if measurement demands it**: `omnibox.rs` `OMNIBOX_CLASS` /
  `focused_class` — e.g. if Edge and Chrome differ, or a class chain is needed
  rather than a single class name
- Modify: `docs/manual-verification-windows.md` — record what the run found
- Create: `plans/reports/verification-260906-<time>-omnibox-guard.md`
- Run: `scripts/probe-omnibox-identity.ps1` (step 3),
  `scripts/verify-windows-omnibox.ps1` (step 4), `scripts/probe-sendinput.ps1`
  (step 2)
- Read only: `scripts/verify-windows-tier*.ps1` — the existing regression suite

## Implementation Steps

1. **Ask before starting.** The harness takes over the screen and types. Idle
   machine, ordinary non-elevated window in the foreground.
2. Confirm injection works here first: `scripts/probe-sendinput.ps1` should
   report `sent=2 err=0` with a non-elevated foreground window.
3. **Measure the real class before testing behaviour.** Run
   `scripts/probe-omnibox-identity.ps1` and focus the address bar in Edge, then
   in Chrome, then a text box in a page — the script prints what UIA says about
   each.
   This is the phase's highest-information step: if it does not equal
   `OMNIBOX_CLASS`, everything downstream is explained and step 4 would have
   read as a pass for the wrong reason.
4. Open a plain tab (`about:blank`) — not a media page — then run
   `scripts/verify-windows-omnibox.ps1`. It reproduces the original condition
   (`Ctrl+A`, Backspace, then the word) and reads the result back through UIA.
   Expect `hồng` on the **first** attempt, which is the one that used to fail.
5. Repeat in Chrome, and in one Electron app (Slack or VS Code) where
   `is_chromium_app` matches but the focus is not an omnibox — expect no
   forward-delete and no lost character.
6. The page-body regression check: a Gmail draft (or any contenteditable),
   caret mid-paragraph, type a Vietnamese word. No character may be lost. This
   is the check the interim disable existed to protect.
7. Read `EMIT took=` from the log across the session; compare against the
   pre-UIA baseline. No new millisecond-scale cost. Confirm no
   `LowLevelHooksTimeout` anywhere in the log.
8. Record everything in a report under `plans/reports/`, and update
   `docs/manual-verification-windows.md`. If step 3 found a mismatch, that is the
   finding — write it up and fix the class matching before re-running 4-7.
9. Mark §3.1 done in `260905-1643/phase-3-correctness.md`, or record precisely
   what still fails.

## Success Criteria

- [ ] `focused_class()`'s real return value is recorded for both Edge and Chrome,
      and `OMNIBOX_CLASS` is confirmed or corrected against it
- [ ] First word into a freshly cleared omnibox renders `hồng`, read back by UIA
- [ ] An Electron app with non-omnibox focus emits no forward-delete
- [ ] A contenteditable mid-paragraph loses no character
- [ ] `EMIT took=` shows no new millisecond-scale cost; no `LowLevelHooksTimeout`
- [ ] A report exists under `plans/reports/`
- [ ] §3.1's status in `260905-1643` reflects the outcome

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| **The class never matches, and that reads as a pass.** The flag stays `false`, the guard never fires, no forward-delete is ever sent, and the only symptom is the mis-render persisting | Step 4 shows `hoồng` while the log shows the guard never fired | Step 3 measures the class *before* behaviour is judged, precisely so a silent no-op cannot be mistaken for a working guard |
| Verification run on a media page, address bar unfocusable, run abandoned as "blocked" | `Ctrl+L`/`Alt+D`/`F6` do nothing | Known limitation from `e486b1b`. Plain tab first; it is step 4's first instruction |
| Typing into a dirty omnibox, which passes trivially | Everything passes first try and the original bug is not exercised | The condition is `Ctrl+A` + Backspace + first word. Only the first word into a cleared field ever failed |
| The harness runs while the user is working | Interrupted session, lost input into the wrong window | Step 1 is asking. This already happened once during research; it is a real failure mode, not politeness |
| UIA in the worker drifts back onto the message loop in a later change | `LowLevelHooksTimeout`, hook removed, typing dies | The worker-thread split is mandatory, not an optimisation (`decisions/0008`). Step 7's timeout check is the regression signal |
| A mismatch is patched by loosening the class match until something passes | Class matching grows special cases per browser version | A mismatch reopens §3.1's design question, with the cheaper freshly-cleared-first-word condition as the comparison |
