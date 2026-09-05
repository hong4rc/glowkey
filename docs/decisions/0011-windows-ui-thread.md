# 0011 — The Windows UI runs on one thread for the life of the process

## Status

Accepted (2026-09-05).

## Context

winit permits one event loop per process and has no reset outside its web
backend. The Windows shell called `eframe::run_native` once per Settings open,
on the hook's own thread, so the second open in a long-running process returned
`RecreationAttempt` and did nothing. About therefore had to be a `MessageBoxW`,
which is modal, plays the system sound, has an OK button, and runs a nested
message loop: the hotkey's deferred indicator refresh and save, posted as thread
messages to the main loop, never ran while About was open, so the VI/EN toggle
looked dead. "Open this window at launch" could not be honored either, because it
would have spent the process's one window at startup.

Research: `plans/reports/researcher-260905-1037-eframe-long-lived-ui-thread.md`.

## Decision

Spawn one thread at startup that runs `run_native` once and never returns, with
`EventLoopBuilderExtWindows::with_any_thread(true)`. Its root viewport is a shim:
one point square, undecorated, parked off-screen, no taskbar entry, never
focused, and it cancels every close. Settings and About are deferred viewports
the root asks for each frame while they should be open; not asking closes them,
asking again reopens them.

The root is off-screen rather than hidden because on egui 0.29.1 on Windows a
hidden viewport stops receiving redraw events and never processes another
command (egui issues #3655, #5229). A hidden root would never drain its queue.

Two message loops now share the process. The hook, the tray and the
thread-local session stay on the main thread. The UI thread receives a
`Settings` snapshot to edit and returns the edited value through a slot plus a
posted thread message; the main thread merges, saves and rebuilds the session,
as it did before. The UI thread never calls `hook::with_session`.

The process ends when `main` returns after the main loop; the UI thread dies
with it.

## Consequences

- Settings reopens any number of times. About is a window with no button and no
  sound, open alone or beside Settings, and the toggles work while it is up.
- "Open this window at launch" is honored on Windows.
- One idle hidden-in-practice window exists for the process's life. egui
  repaints only on request, so its cost at rest is a parked message pump.
- Deferred viewports on Windows 11 with this egui are lightly travelled:
  per-viewport taskbar and focus behaviour is verified by hand, not by a source.
- No egui upgrade. The fix for #3655 post-dates 0.29.1; when the crate moves,
  the shim can become a genuinely hidden root.

## Alternatives rejected

- **Hide and re-show one persistent window.** Broken on this version (#3655).
- **A separate process for the UI.** Works, but adds IPC, a second binary in the
  installer, and a second single-instance story for one settings window.
- **Keep the message box for About.** Keeps every symptom the user reported.
