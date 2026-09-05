---
title: "Windows UI parity: a long-lived UI thread, a real About window, macOS-grade controls"
description: "Move the Windows settings window onto a dedicated eframe thread so it can reopen, give About a real non-modal window (no OK, no sound), make the segmented controls look like macOS, and honor open-at-launch."
status: completed
priority: P1
effort: "1.5-2 days"
tags: [glowkey, windows, ui, egui, about, settings]
created: 2026-09-05
blocks: [260904-2127-glowkey-cross-platform-port]
---

# Windows UI parity

User feedback on the 2026-09-05 10:33 build, verbatim intent: the settings
window now looks like macOS but its borders look wrong; About plays a sound,
is a message box with an OK button, and while it is open the VI/EN toggle
appears dead; the Windows UX should match macOS. Fix all of it.

## Root causes (scouted)

| Symptom | Cause | Evidence |
|---|---|---|
| Borders look wrong | `segmented()` draws a hairline stroke around the track and egui's selected-label paints a blue stroke and white text | `settings_ui.rs::segmented`, egui `SelectableLabel` uses `visuals.selection.stroke` for both border and text |
| About plays a sound | `MessageBoxW(..., MB_ICONINFORMATION)` calls `MessageBeep` | `shell.rs::show_about` |
| About has an OK button, is modal | It is a message box | same |
| VI/EN dead while About open | The hotkey toggle defers indicator refresh and save via `PostThreadMessageW` to the main loop; a message box's modal loop never returns to that loop, so the mode flips silently and the tray glyph stays stale | `hook.rs::wake`, `run_message_loop`, `shell.rs::toggle_mode` |
| Settings opens once per process; open-at-launch inert | winit allows one event loop per process; `settings_ui::show` calls `run_native` per open on the hook thread | `settings_ui.rs::show`, `plans/reports/windows-handoff-260905.md` §2 |

Three of the five share one fix: About and Settings must be real windows that
never block the main thread, which needs the long-lived UI thread the last
handoff deferred.

## Research

- `plans/reports/researcher-260905-1037-eframe-long-lived-ui-thread.md`:
  eframe 0.29.1 can run its loop on a non-main thread via
  `NativeOptions::event_loop_builder` + `EventLoopBuilderExtWindows::with_any_thread(true)`;
  `run_native` blocks that thread for the process's life; **`ViewportCommand::Visible(false)`
  then `Visible(true)` is broken on this version on Windows (egui #3655, #5229)**,
  so the root is created hidden and never toggled; Settings and About are
  `show_viewport_deferred` viewports opened and closed by whether the root calls
  them each frame; `Context::request_repaint()` is thread-safe and wakes the loop;
  idle cost at rest is one parked win32 window. **Implementation note:** the
  same issues mean a *hidden* root never drains its queue either, so the root is
  a visible 1×1 undecorated window parked off-screen (`ui_thread.rs`).
- Segmented control appearance: Apple HIG segmented controls and the Big Sur+
  "switcher" style — a rounded track slightly darker than the window, the selected
  segment raised (white in light, lighter grey in dark) with a soft shadow, no
  hairlines, label text in the normal text colour on every segment.
  `plans/reports/ux-review-260905-0944-shared-settings-layout.md` finding 11.
- macOS About window shape: `app/src/about_window.rs` — 340×180, name bold 22,
  version + commit selectable and secondary, description, credit line, no buttons.

## Decision

Decision 0011 (written in phase 1): the Windows UI runs on one dedicated thread
for the process's life. Taken on the user's instruction to proceed without a
checkpoint (2026-09-05 10:37).

## Phases

| # | Phase | Status | Depends on |
|---|---|---|---|
| 1 | [Long-lived UI thread, Settings as a deferred viewport](./phase-01-start.md) | done; desktop checks open | — |
| 2 | [About window](./phase-02-about-window.md) | done; desktop checks open | 1 |
| 3 | [Segmented control and chrome polish](./phase-03-segmented-control-polish.md) | done; visual check open | — |
| 4 | [Windows UX parity and docs](./phase-04-windows-ux-parity.md) | done; reopen, About, side-by-side and toggle-with-About verified on the live build; hotkey, Esc, sound, look, taskbar, Quit left to the user | 1, 2, 3 |

## Acceptance criteria

1. Settings opens, closes and reopens any number of times in one process; edits
   still merge and save exactly as `shell::open_settings` does today; the hook
   keeps typing while it is open.
2. About is a window: icon, name, version with commit, description, credit line,
   the elevated-windows note; no button; no sound; non-modal; closable from the
   title bar and Esc; openable alongside Settings.
3. With About open: the hotkey toggle flips VI/EN and the tray glyph updates at
   once; the tray menu toggle does too.
4. Segmented controls and the tab strip have no hairline borders; the selected
   segment is raised and its text is the normal text colour; light and dark.
5. "Open this window at launch" opens Settings at startup on Windows.
6. `cargo test -p glowkey`, `cargo clippy --all-targets -D warnings` (Windows)
   and `cargo clippy --target aarch64-apple-darwin -D warnings` green; nothing on
   the hook path calls into the UI thread.

## Non-goals

- No hotkey recorder on Windows.
- No egui upgrade.
- No change to the macOS windows.

## Risks

- Deferred viewports on Windows 11 with this egui: per-viewport taskbar entry
  and focus behaviour unverified (research §Q6). Signal: About or Settings has
  no taskbar entry or steals focus oddly. Response: try `with_taskbar`, else
  accept and record.
- Two message loops in one process. The tray and hook stay on the main thread;
  the UI thread must never call `hook::with_session` (thread-local). Signal: a
  `None` from `with_session` on the UI thread. Response: route through the
  result channel only.
- Quit path: root never closes; tray Quit ends the main loop, `main` returns,
  and returning from `main` ends the process (`ExitProcess`), UI thread included.
  Signal: a zombie GlowKey after Quit. Response: a `Quit` command that lets the
  root close, joined with a timeout.

## Rollback

Revert the branch. Phase 3 is independent of 1 and 2 and can land alone.
