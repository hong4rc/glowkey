---
phase: 6
title: "Windows verification on real hardware"
status: pending
priority: P1
effort: "2d"
dependencies: [5]
---

# Phase 6: Windows verification on real hardware

## Overview

The phase that decides whether any of this works. Everything before it is a
hypothesis; `cargo check` proves types, not behaviour. **This phase is executed by a
human at a Windows machine** and its output is a limitations list, not a green tick.

## Requirements

- Functional: Vietnamese typing verified in real applications, not a test harness.
- Functional: every failure found is either fixed or written down as a limitation.
- Non-functional: no claim of support for an environment that was not typed in.

## Architecture

Not code — a protocol. Add these as `docs/manual-verification-windows.md`, in the
shape of the existing macOS checklist.

### Tier 1 — the model holds

- [ ] `hoongf` → `hồng` in Notepad
- [ ] Synthesized input is **not** reprocessed: no doubled characters, no runaway.
      This is the tag guard; if it fails, nothing else matters.
- [ ] Every letter is suppressed and re-emitted — no path where the original
      character lands *and* a replacement is injected
- [ ] `hoongf` ⌫ `z` → `hôn` (mid-word)
- [ ] `hồng` ␣ ⌫ `z` → `hông` (boundary re-composition)
- [ ] `hoongf,` ␣ ⌫⌫ `z` → `hông` (the double-boundary case)
- [ ] `exit`␣ → `exit`, not `eĩt` (auto-fix and boundary replay)
- [ ] Tone changes, capitalization, VNI, Simple Telex, Quick Telex, brackets

### Tier 2 — the applications that break input methods

- [ ] **Notepad** — the baseline
- [ ] **Chrome / Edge** — and specifically the **address bar**. Measure whether the
      trailing-selection defect the macOS AX guard exists for reproduces here. If it
      does not, say so and do not port the guard.
- [ ] **Windows Terminal** — must be excluded by default and must stay excluded
- [ ] **VS Code** — Electron; the macOS race showed up in exactly this class
- [ ] **An Electron app** (Slack, Discord) — multiprocess renderer path
- [ ] **Word / a native Win32 editor**
- [ ] **An elevated window** (Task Manager) — must fail *visibly*, not silently

### Tier 3 — the blind model's edges

- [ ] Arrow keys, Home/End, mouse click mid-word all flush; the next letter does not
      eat text
- [ ] Alt-Tab and application switching update the frontmost app
- [ ] Hotkeys: mode toggle, per-app toggle, ⌃⇧W correction
- [ ] A custom hotkey recorded on Windows matches on Windows
- [ ] Excluded apps type plain keys
- [ ] Timing: log the equivalent of `EMIT took=`; a maximum in the tens of
      milliseconds means something blocking got into the callback

## Related Code Files

- Create: `docs/manual-verification-windows.md`
- Create: `plans/reports/windows-verification-<date>.md` — the recorded results
- Modify: `docs/handoff.md` — a Windows status section

## Implementation Steps

1. Build on the Windows machine: `cargo build --release --target x86_64-pc-windows-msvc`.
2. Run Tier 1 first. **If the tag guard fails, stop** — everything downstream is
   noise until injection stops re-entering the hook.
3. Run Tier 2, recording exact behaviour per application, including the ones that
   work.
4. Run Tier 3.
5. Write the results file. Every unchecked box becomes either a fix or a documented
   limitation with a reason.
6. Feed anything structural back as a decision record.

## Success Criteria

- [ ] Tier 1 fully green — this is non-negotiable, it is the model itself
- [ ] Tier 2 recorded for every listed application, pass or fail
- [ ] Tier 3 recorded
- [ ] `plans/reports/windows-verification-<date>.md` exists with real observations
- [ ] A written limitations list, including the elevated-window case
- [ ] The Chrome address-bar question answered with evidence either way

## Risk Assessment

**The most likely failure is injection ordering in Chrome or Electron**, because
that is exactly where macOS failed and for structural reasons that are not
macOS-specific. *Signal:* `hoongf` → `hoồng`, or the first transform after a letter
landing wrong. *Response:* the macOS fix was total suppression plus one ordered
queue, which this port already carries; if it still races, the cause is different
and needs measuring before fixing.

**The second most likely is a blocked callback.** *Signal:* input freezes, or the
hook silently stops firing. *Response:* Windows removes a slow hook rather than
warning. Check `LowLevelHooksTimeout` behaviour explicitly; treat any hook removal
as a P1 defect, not a tuning issue.

**A tester can produce a false pass** by testing only Notepad. *Signal:* Tier 2 is
skipped as "probably fine". *Response:* Tier 2 is the phase. Notepad proves almost
nothing — every input method works in Notepad.
