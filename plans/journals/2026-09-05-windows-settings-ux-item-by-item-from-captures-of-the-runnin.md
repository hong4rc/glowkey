---
title: "Windows settings UX, item by item, from captures of the running window"
date: 2026-09-05
summary: "List editors became windows, one control axis and an even rhythm, a hotkey popup, keycaps, keyboard focus for the segmented control; review caught arrow keys stealing focus."
---

# Windows settings UX, item by item, from captures of the running window

## What happened

"Improve UI/UX each item" needed to start from what the Windows settings window
actually looked like, so instead of reasoning from code the session captured
it: `PrintWindow` renders a window to a bitmap without focus or input, and
egui accepts mouse clicks as window messages, so tabs could be switched by
posting a move/down/up to the window itself. Seven captures produced a
fifteen-row findings table (`plans/260905-1145-windows-settings-ux-polish/plan.md`).

Built in four phases on one branch: the three list editors became deferred
viewports of their own (they had been `egui::Window` overlays that covered the
tabs and swallowed clicks); checkboxes moved into the control column so every
control and caption shares one axis; row, caption and section gaps became
three edge-to-edge constants; the toggle hotkey became a popup because three
"Ctrl+Shift+Space" segments did not fit; the fixed shortcut row draws keycaps;
list rows say "20 apps"; About grew a Copy under the version; and the
hand-painted segmented control gained keyboard focus, arrow keys, a focus
ring and screen-reader names.

## What did not survive contact

- Keycaps inside running caption text inflated the line and broke it oddly.
  Reverted after the capture showed it; captions are text.
- The list window never appeared on first build: the Manage button flipped a
  flag inside the settings viewport, but only the root asks for windows and
  nothing repainted it. Same lesson as About and Settings closing.
- A slowed-down posted click (move, 120 ms, press) stopped working: after a
  posted `WM_MOUSEMOVE`, Windows notices the real cursor is elsewhere and sends
  `WM_MOUSELEAVE`, so the press arrives with no pointer. Rapid sequences work.
- Headless egui embeds child viewports, so their ids never reach the output;
  the host records what it asked for in test builds instead.
- egui drops focus from any id not drawn as a widget that frame, so
  "interested in focus" alone did nothing; the track became the focusable
  widget.

## What the review caught

egui maps arrow keys to cardinal focus moves in `Focus::begin_pass`, before any
widget code runs, so Right on the focused control also handed focus to the
Manage button below-right. `consume_key` would have been too late; the fix is
the focus-lock filter sliders use, which takes hold one frame after focus is
gained. Also: every segment had been its own Tab stop; the focusable track had
no screen-reader name; Manage… on an open window did nothing visible; the macro
import box was unreachable at the minimum window size; the section gap measured
22 not 18 because the row gap stacked on it.

## Next steps

- Desktop checks for the user: Tab and arrows on the controls, the popup,
  taskbar entries for list windows, Copy in About, dark theme.
- macOS build of the shared-spec window still unrun.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
