---
phase: 2
title: "Windows Tier 5 desktop checks"
status: completed
priority: P2
effort: "2-3h"
dependencies: []
---

# Phase 2: Windows Tier 5 desktop checks

## Overview

Run the twenty-odd boxes of `docs/manual-verification-windows.md` Tier 5 against
a build of `main` on this machine, fix what they find, and write the result to
`plans/reports/windows-verification-260905.md`. This is the stand-in for the
macOS runtime pass the session cannot do, and it is the first time most of these
boxes have been touched since the engine split.

## Requirements

- Functional: every Tier 5 box ends the phase either ticked with evidence or
  unticked with a written reason. A fault found is diagnosed before it is
  fixed, and any fix is a separate, revertable edit.
- Non-functional: **no synthetic keystrokes into the live session, ever.**
  Window messages are posted to GlowKey's own windows only, the way plan
  `260905-1145` phase 4 took its captures. GlowKey is a keyboard hook: the
  process being under test is in the path of everything the user types, so the
  build under test is started and stopped deliberately and never left running
  past the phase.

## Architecture

Three kinds of box, and the boundary between them is the deliverable's honesty:

**Agent-runnable, no input at all.** Registry state
(`reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"` before and
after toggling start-at-login), the settings file under `%APPDATA%\GlowKey\`,
the log under `%LOCALAPPDATA%` and its rotation, process lifetime after tray
Quit (`tasklist` / `Get-Process GlowKey`), the absence of a console window,
idle CPU and working set with the settings window closed (`Get-Process` twice,
a minute apart, recorded as numbers), and a settings file copied from a Mac
still loading.

**Agent-runnable through GlowKey's own windows.** Reopening Settings three
times, About opening and closing by Esc and by X, the two windows side by side
with their own taskbar entries, the segmented-control appearance in both
themes, `Manage…` opening each list window, and the control-column alignment —
all reachable by posting messages to GlowKey's windows and capturing with
`PrintWindow`, exactly as `260905-1145` did. Dark theme is captured by flipping
`AppsUseLightTheme` for the capture and restoring it after.

**User-owned, and named as such in the report.** Anything that needs a real
keypress into the live session: `Ctrl+Shift+Space` with About open, the tray
menu toggle, tab/arrow-key focus movement through the segmented controls, the
hotkey popup opening with Space/Enter, the clipboard tools, and an edit typed
into the third Settings open. These are left unticked with the reason
"needs a physical keypress; agent must not synthesise input", written as a short
list the user can walk in a few minutes.

Two Tier 5 items deserve their own care. **Idle cost** is the check on the
`winit`+`egui` decision and is meaningless with the window open; `decisions/0011`
added an off-screen one-point shim window for the process's life, and the
recorded number is the evidence that it did not move — so the report carries the
number, not a tick. **A settings file copied from a Mac still loads** is the
byte-compatibility claim `0012` makes about `Settings` moving crates; build the
fixture from the macOS shape rather than from a Windows file with a field
renamed.

## Related Code Files

- Read: `docs/manual-verification-windows.md` (Tier 5, and "Recording the
  results" for the report's required shape).
- Read: `app/src/platform/windows/{ui_thread,settings_ui,about_ui,tray,startup}.rs`
  when a box fails, to locate the cause before the fix.
- Create: `plans/reports/windows-verification-260905.md`.
- Modify: only what a failing box proves is broken.

## Implementation Steps

1. Build the shipping shape (`just` recipe or `cargo build --release -p
   glowkey`; check the `justfile` for the Windows recipe) and confirm the binary
   under test is from `main` at the engine-split commit.
2. Record the pre-state that the run will disturb: the `Run` registry value, the
   existing `%APPDATA%\GlowKey\settings.json`, and `AppsUseLightTheme`. Restore
   all three at the end.
3. Start GlowKey. Take the idle measurement first, with no window open, before
   anything else has touched the process.
4. Walk the agent-runnable boxes in the order of the Tier 5 list, capturing as
   you go into the report's evidence column.
5. Walk the window-message boxes: Settings open/close/reopen ×3, About by Esc
   and by X, side-by-side taskbar entries, the three `Manage…` windows and their
   Esc, closing Settings closing them, and the alignment and segmented-control
   captures in both themes.
6. Tray Quit; confirm no `GlowKey.exe` remains.
7. For each failure: diagnose to a cause in the source before writing a fix, fix
   it, re-run that box and the gate list.
8. Write the report; list the user-owned boxes at the end as a short walkable
   checklist.
9. Restore the registry value, settings file and theme setting from step 2.

## Success Criteria

- [x] Every Tier 5 box is ticked with evidence, or unticked with a reason.
- [x] Idle CPU and working set are recorded as numbers, taken with no window
      open, and compared against any figure an earlier report holds (none
      existed; this run is the baseline: ≈2.0% of one core, ~101 MB).
- [x] A Mac-shaped `settings.json` loads without loss.
- [x] Start-at-login adds the `Run` value and **disabling removes it**, shown by
      `reg query` output before and after.
- [x] Tray Quit leaves no `GlowKey.exe`.
- [x] `plans/reports/windows-verification-260905.md` exists in the shape
      "Recording the results" asks for (appended as a new section; the file
      already held an unrelated Tier 1 run from earlier the same day).
- [x] The user-owned boxes are listed for the user, not silently ticked.
- [x] The machine is left as it was found: registry, settings file, theme.

## Outcome

Done. No engine-split defect was found — every failure this phase hit was in
the verification technique, not the product: posted `WM_MOUSEMOVE` /
`WM_LBUTTONDOWN` / `WM_LBUTTONUP` messages do not reach an egui viewport under
the `decisions/0011` persistent-UI-thread architecture, while posted keyboard
messages and `WM_COMMAND` to the tray both do. That blocked tab switching,
`Manage…`, and any in-window click, and those boxes are left unticked with
that reason in `plans/reports/windows-verification-260905.md` rather than
faked. Nothing was fixed because nothing in the product was proven broken.

## Risk Assessment

- **The build under test intercepts the user's typing.** It is a keyboard hook.
  *Signal:* the phase is running at all. *Response:* start it deliberately, do
  the run, quit it; never leave it running past the phase, and never synthesise
  input while it lives.
- **A box fails for a reason the engine split introduced.** Plausible: this is
  the first desktop run since it landed. *Signal:* a settings, exclusion or
  toggle box misbehaves. *Response:* diagnose against `decisions/0012`'s
  layering before assuming a UI fault — the settings file moved crates and the
  exclusion merge rule moved with it.
- **The theme flip or the registry write is left behind.** *Signal:* step 9 is
  skipped because an earlier step failed. *Response:* step 2 records the
  pre-state to a file, so restoration does not depend on remembering it.
- **A "fix" that only makes a box tick.** *Response:* the rule from step 7 —
  cause before fix, and the fix gets its own commit.
