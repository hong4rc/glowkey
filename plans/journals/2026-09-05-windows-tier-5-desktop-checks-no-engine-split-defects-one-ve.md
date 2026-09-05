---
title: "Windows Tier 5 desktop checks: no engine-split defects, one verification-technique gap"
date: 2026-09-05
summary: "Ran the Windows Tier 5 shell checks via posted window messages only; found no product defects, but posted mouse messages don't reach egui viewports under the decisions/0011 UI-thread architecture (keyboard and tray WM_COMMAND do). Closed out plan 260905-1519."
---

# Windows Tier 5 desktop checks: no engine-split defects, one verification-technique gap

## What happened

Session ran on Windows, picking up plan `260905-1519-macos-renderer-parity-and-windows-tier5`
where phase 1 (macOS renderer parity, ported from `260905-1145`) had already
landed on `main` at `c348714`. Phases 2 and 3 were this session's job: run the
Tier 5 desktop checks from `docs/manual-verification-windows.md` — the
stand-in for the macOS runtime pass this machine can't do — and then gate,
document, and merge.

Built the release binary at `c348714`, recorded the pre-state (the `Run`
registry value, `settings.json`, `AppsUseLightTheme`), and walked the Tier 5
list under one hard rule: no synthetic keystrokes or mouse input into the live
session, ever. Two channels turned out to work for driving the app without
touching the desktop:

- `WM_COMMAND` posted straight to the tray's own message window
  (`GlowKeyTray`), using the same command ids its context menu uses. The
  handler at `tray.rs:669` was wired for exactly this ("kept for
  completeness... nothing routes through here in practice" — it does now).
- `WM_CLOSE` / `WM_KEYDOWN(VK_ESCAPE)` posted directly to a viewport's own
  `HWND` (found by title via `EnumWindows` filtered to GlowKey's PID).

That combination reopened Settings three times, opened/closed About by both
`WM_CLOSE` and posted `Esc`, put both windows on screen together, exercised
the mode toggle with About open (indicator updated 1 ms later), captured both
themes, and confirmed Tray Quit leaves no process.

**The one channel that did not work:** posted `WM_MOUSEMOVE` /
`WM_LBUTTONDOWN` / `WM_LBUTTONUP` at correctly computed client coordinates
inside a Settings viewport produced no effect at all — not even a hover
highlight — across repeated attempts (checkbox, segmented control, tab strip;
both `PostMessageW` and synchronous `SendMessageW`). Posted keyboard messages
to the *same* window worked fine, so the window is receiving messages in
general; this is specifically pointer input into an egui viewport under the
`decisions/0011` persistent-UI-thread / deferred-viewport architecture. The
`260905-1145` phase 4 report used posted mouse clicks successfully against the
pre-0011, per-open `eframe` model — so this may be new since that decision,
not a technique mistake repeated from before. It blocked tab switching,
`Manage…` on the three list windows, and any in-window click; those boxes are
left unticked in the report with that reason, not faked. A real mouse still
drives the app normally — this is a verification-technique gap, not a product
defect.

No engine-split (`decisions/0012`) defect turned up anywhere: settings
persistence, the Mac-shaped fixture, start-at-login's registry round-trip, log
rotation, and the exclusion/toggle paths all behaved. One incidental fix: a
trailing blank line `cargo fmt --all -- --check` caught in
`app/src/prefs/tabs.rs`, left over from phase 1's edits and outside the six
gates that phase 1 actually ran — fixed in its own commit before the gate run.

Recorded the first-ever Windows idle CPU/working-set baseline: ≈2.0% of one
core, ~101 MB, with no window open — nothing to compare it against yet, but
now there is a number.

## Decision

No decision changed. `docs/decisions/0010` and `0012` were checked against
what phase 1 and phase 2 found and needed no edit — `ListId::unit` is
consistent with the spec already owning row content, and no engine-split
defect surfaced to revise `0012`'s claims. The mouse-input gap is left as an
open question for whoever next touches `platform/windows/ui_thread.rs`,
not a new ADR — nothing here says what should replace the current model, only
that today's verification technique can't drive it.

## Next steps

- The macOS runtime pass (`docs/handoff.md` §11 item 1) is still outstanding
  and now carries three more things to watch, from phase 1's compile-only
  parity work: the checkbox control-column indent, the count units, and the
  row rhythm constants.
- Whoever next touches the Windows UI thread: work out why posted mouse
  messages don't reach an egui viewport here, or whether enabling
  `accesskit`/UIA support would let a future pass drive Settings without a
  person at the keyboard.
- User-owned Tier 5 boxes are listed at the end of
  `plans/reports/windows-verification-260905.md`: `Ctrl+Shift+Space` with
  About open, the tray icon's own click, Tab/←/→ through the segmented
  controls, and the three clipboard tools.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
