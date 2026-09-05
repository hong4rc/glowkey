---
title: "Windows UI on one thread: Settings reopens, About is a window, and two review catches"
date: 2026-09-05
summary: eframe now runs once on a dedicated thread with an off-screen root; Settings and About are deferred viewports. Review caught a wrong-viewport wake and an un-normalized merge baseline.
---

# Windows UI on one thread: Settings reopens, About is a window, and two review catches

## What happened

The user saw the first spec-rendered Windows settings window and reported: the
borders look wrong, About plays a sound and is a message box with an OK button,
and the VI/EN toggle seems dead while About is open. Scouting showed three of
the five symptoms shared one cause: winit allows one event loop per process, so
Settings ran `run_native` once on the hook thread and About had to be a modal
`MessageBoxW`, whose nested loop never let the main loop turn. The hotkey's
deferred indicator refresh therefore sat in the queue while About was up; the
mode flipped silently and the tray glyph went stale.

Research (`plans/reports/researcher-260905-1037-eframe-long-lived-ui-thread.md`)
confirmed eframe 0.29 can run on a dedicated thread with `with_any_thread`, and
found the trap: on this egui on Windows a hidden viewport stops receiving redraw
events and never processes another command (egui #3655, #5229). Implication,
beyond the researcher's own recommendation: a hidden root would never drain its
command queue either. The root is therefore a visible 1×1 undecorated window
parked off-screen with no taskbar entry and no focus, and Settings and About are
deferred viewports the root asks for each frame.

Built in one pass: `ui_thread.rs`, `about_ui.rs`, a `SettingsApp` that hands its
result back through a slot and a posted thread message, `shell::apply_settings`
on the main thread, `hook::wake_main_loop`, open-at-launch honoured, and a
hand-painted segmented control (soft track, raised selection with shadow, no
hairlines, labels in the text colour) that also became the tab strip.

## What the review caught

- `Context::request_repaint()` from another thread targets whichever viewport is
  on the context's stack at that instant, which can be a child mid-frame. Tray
  commands could then sit undrained. Fixed by waking the root by name.
- The baseline crossing the thread was the raw file order, while `finalize`
  compares against the normalized exclusion list. Every close would have read as
  an exclusions edit and overwritten an app the tray excluded while the window
  was open — the exact loss the merge exists to prevent, on the one field the
  hotkey writes. The existing merge test never caught it because it built the
  edited value by hand. Fixed with a normalized baseline and a test that runs the
  real path through `finalize`.
- A reopen arriving in the same frame as a close was swallowed; a result
  delivered after WM_QUIT was dropped. Both fixed.

The tester found egui's headless click needs the pointer move in its own frame
before the press; the minimal repro settled it in two minutes.

## Decision

Decision 0011: the Windows UI runs on one dedicated thread for the life of the
process. Taken on the user's instruction to proceed without a checkpoint.

## Next steps

- Desktop checks the user must do (no synthetic input into the live session):
  reopen Settings three times, About with Esc and X, both windows at once,
  Ctrl+Shift+Space with About open, tray Quit leaves no process. Listed in
  `docs/manual-verification-windows.md` Tier 5.
- The off-screen root receiving redraws is the one unverified assumption that
  gates everything; the startup log shows Settings opening at launch through it,
  which is the first evidence it works.
- macOS: the spec-rendered AppKit window is still unrun.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
