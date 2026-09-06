---
phase: 5
title: "macOS runtime verification"
status: pending
priority: P1
effort: "1d"
dependencies: []
---

# Phase 5 — macOS runtime verification

**Blocked on:** a Mac. Nothing else.
**Gates:** phase 3.3, phase 7's macOS-side refactors, phase 6's install docs, and
any claim in `README.md` that macOS is feature-complete.

## Why this is a phase and not a checklist item

The engine split (`decisions/0012`), the shared settings spec (`0010`) and the
Windows UI thread (`0011`) all landed on `main` on 2026-09-05. `handoff.md:702-703`
says the macOS side of all of it is **compile-checked only and has never been
run**. The session that did that work ran on Windows and used the Windows Tier 5
checks as a stand-in.

So macOS is in an unusual state: its code was restructured under it, its headless
suite passes, and no human has watched it type since. `README.md:26` calls macOS
"feature-complete against the useful Unikey/EVKey set". That claim is not
currently backed by a run, and the audits flagged it as the project's main
over-claim.

Two kinds of parity were conflated across the audits, and both are true:

- **Layout-constant parity is real** — the 6/10/18 rhythm, checkbox column and
  count units were read in both renderers and match.
- **Runtime parity is unestablished** — nothing has been observed.

## What to run

`docs/manual-verification.md`, which has **never been run end to end**. Treat
unticked sections as unverified. Beyond it, `handoff.md §11.1` names the specific
things this restructuring put at risk:

1. The spec-rendered Settings window: all four tabs, the list editors, hotkey
   recording.
2. ⌃⇧Space; ⌃⇧E in Ghostty (the "VI ⚠" HUD).
3. **⌃⇧W with the Personal Words window open** — a review found and fixed a
   blanked list there and nobody has watched it work.
4. The log reads `KEY` before `TOGGLE mode`, as it now does on Windows.
5. The three renderer-parity details that could only be compile-checked: the
   checkbox sits in the control column with its caption under the title (not the
   box); the count rows read `20 apps` / `0 macros` / `0 words` and their
   Vietnamese forms; row rhythm is 6 control-to-caption, 10 row-to-row, 18 before
   a section header.

## Long-standing unverified fixes this pass should close

- The §6.9 system-wide freeze fix has never been verified live.
- Accessibility revoked while running (§6.6) — including whether some macOS
  versions terminate the process outright, which would make the recovery path
  unreachable and harmless.
- The omnibox `EMIT took=` number in §7 is an estimate, not a measurement.
  Needs a granted build and someone typing in Chrome.
- Whether Gatekeeper on macOS 26 actually shows "damaged" for the ad-hoc DMG —
  this wording gates the install documentation in phase 6.

## Also needing a machine (Windows, same class of blocker)

- ~~The Gmail mid-paragraph test that decides phase 3.1.~~ **Answered
  2026-09-06**: the surviving `hoồng` is the address bar only, and page bodies
  are clean. Still worth one deliberate Gmail run when convenient, as the
  acceptance check for 3.1's *real* guard rather than as an open question.
- The League `B` test that decides phase 3.4.
- A dead-hook reproduction to validate phase 3.2's watchdog.
- Windows Tier 1 re-run with EVKey **stopped** — the only verification round so
  far was contaminated by a running EVKey, which transformed text while GlowKey
  was excluded and made results look inconsistent until someone noticed.
- Tier 2: Chrome, Windows Terminal, VS Code, Electron, elevated windows,
  dead-key layouts, AltGr.
- What the recorded ~2% idle CPU / ~101 MB working set is doing.

## Safety when verifying on Windows

Do not send synthetic keystrokes into a live session — the harnesses steal focus
and type. Ask first, or use `scripts/verify-windows-isolated.ps1` (written, not
yet verified).

## Acceptance

`docs/manual-verification.md` ticked end to end at least once, with the date and
the build's commit recorded, and `README.md`'s verified-state claims rewritten to
match what was actually observed.
